use crate::{Authentication, Error, IntoTargetAddr, Result, TargetAddr, ToProxyAddrs};
use futures::{
    stream,
    stream::Fuse,
    task::{Context, Poll},
    Stream, StreamExt,
};
use std::{
    borrow::Borrow,
    io,
    net::{Ipv4Addr, Ipv6Addr, SocketAddr},
    ops::{Deref, DerefMut},
    pin::Pin,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;

#[repr(u8)]
#[derive(Clone, Copy)]
enum Command {
    Connect = 0x01,
    Bind = 0x02,
    #[allow(dead_code)]
    Associate = 0x03,
    #[cfg(feature = "tor")]
    TorResolve = 0xF0,
    #[cfg(feature = "tor")]
    TorResolvePtr = 0xF1,
}

/// A SOCKS5 client.
///
/// For convenience, it can be dereferenced to `tokio_tcp::TcpStream`.
#[derive(Debug)]
pub struct Socks5Stream {
    tcp: TcpStream,
    target: TargetAddr<'static>,
}

impl Deref for Socks5Stream {
    type Target = TcpStream;

    fn deref(&self) -> &Self::Target {
        &self.tcp
    }
}

impl DerefMut for Socks5Stream {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.tcp
    }
}

impl Socks5Stream {
    /// Connects to a target server through a SOCKS5 proxy.
    ///
    /// # Error
    ///
    /// It propagates the error that occurs in the conversion from `T` to
    /// `TargetAddr`.
    pub async fn connect<'t, P, T>(proxy: P, target: T) -> Result<Self>
    where
        P: ToProxyAddrs,
        T: IntoTargetAddr<'t>,
    {
        Self::execute_command(proxy, target, Authentication::None, Command::Connect).await
    }

    /// Connects to a target server through a SOCKS5 proxy using given username
    /// and password.
    ///
    /// # Error
    ///
    /// It propagates the error that occurs in the conversion from `T` to
    /// `TargetAddr`.
    pub async fn connect_with_password<'a, 't, P, T>(
        proxy: P,
        target: T,
        username: &'a str,
        password: &'a str,
    ) -> Result<Self>
    where
        P: ToProxyAddrs,
        T: IntoTargetAddr<'t>,
    {
        Self::execute_command(
            proxy,
            target,
            Authentication::Password { username, password },
            Command::Connect,
        )
        .await
    }

    fn validate_auth<'a>(auth: &Authentication<'a>) -> Result<()> {
        match auth {
            Authentication::Password { username, password } => {
                let username_len = username.as_bytes().len();
                if username_len < 1 || username_len > 255 {
                    Err(Error::InvalidAuthValues("username length should between 1 to 255"))?
                }
                let password_len = password.as_bytes().len();
                if password_len < 1 || password_len > 255 {
                    Err(Error::InvalidAuthValues("password length should between 1 to 255"))?
                }
            }
            Authentication::None => {}
        }
        Ok(())
    }

    #[cfg(feature = "tor")]
    pub async fn tor_resolve<'t, P, T>(proxy: P, target: T) -> Result<TargetAddr<'static>>
    where
        P: ToProxyAddrs,
        T: IntoTargetAddr<'t>,
    {
        let sock = Self::execute_command(proxy, target, Authentication::None, Command::TorResolve).await?;

        Ok(sock.target_addr().to_owned())
    }

    #[cfg(feature = "tor")]
    pub async fn tor_resolve_ptr<'t, P, T>(proxy: P, target: T) -> Result<TargetAddr<'static>>
    where
        P: ToProxyAddrs,
        T: IntoTargetAddr<'t>,
    {
        let sock = Self::execute_command(proxy, target, Authentication::None, Command::TorResolvePtr).await?;

        Ok(sock.target_addr().to_owned())
    }

    async fn execute_command<'a, 't, P, T>(
        proxy: P,
        target: T,
        auth: Authentication<'a>,
        command: Command,
    ) -> Result<Socks5Stream>
    where
        P: ToProxyAddrs,
        T: IntoTargetAddr<'t>,
    {
        Self::validate_auth(&auth)?;

        let sock = SocksConnector::new(auth, command, proxy.to_proxy_addrs().fuse(), target.into_target_addr()?)
            .execute()
            .await?;

        Ok(sock)
    }

    /// Consumes the `Socks5Stream`, returning the inner `tokio_tcp::TcpStream`.
    pub fn into_inner(self) -> TcpStream {
        self.tcp
    }

    /// Returns the target address that the proxy server connects to.
    pub fn target_addr(&self) -> TargetAddr<'_> {
        match &self.target {
            TargetAddr::Ip(addr) => TargetAddr::Ip(*addr),
            TargetAddr::Domain(domain, port) => {
                let domain: &str = domain.borrow();
                TargetAddr::Domain(domain.into(), *port)
            }
        }
    }
}

/// A `Future` which resolves to a socket to the target server through proxy.
pub struct SocksConnector<'a, 't, S> {
    auth: Authentication<'a>,
    command: Command,
    proxy: Fuse<S>,
    target: TargetAddr<'t>,
    buf: [u8; 513],
    ptr: usize,
    len: usize,
}

impl<'a, 't, S> SocksConnector<'a, 't, S>
where
    S: Stream<Item = Result<SocketAddr>> + Unpin,
{
    fn new(auth: Authentication<'a>, command: Command, proxy: Fuse<S>, target: TargetAddr<'t>) -> Self {
        SocksConnector {
            auth,
            command,
            proxy,
            target,
            buf: [0; 513],
            ptr: 0,
            len: 0,
        }
    }

    /// Connect to the proxy server, authenticate and issue the SOCKS command
    pub async fn execute(&mut self) -> Result<Socks5Stream> {
        let next_addr = self.proxy.select_next_some().await?;
        let mut tcp = TcpStream::connect(next_addr)
            .await
            .map_err(|_| Error::ProxyServerUnreachable)?;

        self.authenticate(&mut tcp).await?;

        // Send request address that should be proxied
        self.prepare_send_request();
        tcp.write_all(&self.buf[self.ptr..self.len]).await?;

        let target = self.receive_reply(&mut tcp).await?;

        Ok(Socks5Stream { tcp, target })
    }

    fn prepare_send_method_selection(&mut self) {
        self.ptr = 0;
        self.buf[0] = 0x05;
        match self.auth {
            Authentication::None => {
                self.buf[1..3].copy_from_slice(&[1, 0x00]);
                self.len = 3;
            }
            Authentication::Password { .. } => {
                self.buf[1..4].copy_from_slice(&[2, 0x00, 0x02]);
                self.len = 4;
            }
        }
    }

    fn prepare_recv_method_selection(&mut self) {
        self.ptr = 0;
        self.len = 2;
    }

    fn prepare_send_password_auth(&mut self) {
        if let Authentication::Password { username, password } = self.auth {
            self.ptr = 0;
            self.buf[0] = 0x01;
            let username_bytes = username.as_bytes();
            let username_len = username_bytes.len();
            self.buf[1] = username_len as u8;
            self.buf[2..(2 + username_len)].copy_from_slice(username_bytes);
            let password_bytes = password.as_bytes();
            let password_len = password_bytes.len();
            self.len = 3 + username_len + password_len;
            self.buf[(2 + username_len)] = password_len as u8;
            self.buf[(3 + username_len)..self.len].copy_from_slice(password_bytes);
        } else {
            unreachable!()
        }
    }

    fn prepare_recv_password_auth(&mut self) {
        self.ptr = 0;
        self.len = 2;
    }

    fn prepare_send_request(&mut self) {
        self.ptr = 0;
        self.buf[..3].copy_from_slice(&[0x05, self.command as u8, 0x00]);
        match &self.target {
            TargetAddr::Ip(SocketAddr::V4(addr)) => {
                self.buf[3] = 0x01;
                self.buf[4..8].copy_from_slice(&addr.ip().octets());
                self.buf[8..10].copy_from_slice(&addr.port().to_be_bytes());
                self.len = 10;
            }
            TargetAddr::Ip(SocketAddr::V6(addr)) => {
                self.buf[3] = 0x04;
                self.buf[4..20].copy_from_slice(&addr.ip().octets());
                self.buf[20..22].copy_from_slice(&addr.port().to_be_bytes());
                self.len = 22;
            }
            TargetAddr::Domain(domain, port) => {
                self.buf[3] = 0x03;
                let domain = domain.as_bytes();
                let len = domain.len();
                self.buf[4] = len as u8;
                self.buf[5..5 + len].copy_from_slice(domain);
                self.buf[(5 + len)..(7 + len)].copy_from_slice(&port.to_be_bytes());
                self.len = 7 + len;
            }
        }
    }

    fn prepare_recv_reply(&mut self) {
        self.ptr = 0;
        self.len = 4;
    }

    async fn password_authentication_protocol(&mut self, tcp: &mut TcpStream) -> Result<()> {
        self.prepare_send_password_auth();
        tcp.write_all(&self.buf[self.ptr..self.len]).await?;

        self.prepare_recv_password_auth();
        tcp.read_exact(&mut self.buf[self.ptr..self.len]).await?;

        if self.buf[0] != 0x01 {
            return Err(Error::InvalidResponseVersion);
        }
        if self.buf[1] != 0x00 {
            return Err(Error::PasswordAuthFailure(self.buf[1]));
        }

        Ok(())
    }

    async fn authenticate(&mut self, tcp: &mut TcpStream) -> Result<()> {
        // Write request to connect/authenticate
        self.prepare_send_method_selection();
        tcp.write_all(&self.buf[self.ptr..self.len]).await?;

        // Receive authentication method
        self.prepare_recv_method_selection();
        tcp.read_exact(&mut self.buf[self.ptr..self.len]).await?;
        if self.buf[0] != 0x05 {
            return Err(Error::InvalidResponseVersion);
        }
        match self.buf[1] {
            0x00 => {
                // No auth
            }
            0x02 => {
                self.password_authentication_protocol(tcp).await?;
            }
            0xff => {
                return Err(Error::NoAcceptableAuthMethods);
            }
            m if m != self.auth.id() => return Err(Error::UnknownAuthMethod),
            _ => unimplemented!(),
        }

        Ok(())
    }

    async fn receive_reply(&mut self, tcp: &mut TcpStream) -> Result<TargetAddr<'static>> {
        self.prepare_recv_reply();
        self.ptr += tcp.read_exact(&mut self.buf[self.ptr..self.len]).await?;
        if self.buf[0] != 0x05 {
            return Err(Error::InvalidResponseVersion);
        }
        if self.buf[2] != 0x00 {
            return Err(Error::InvalidReservedByte);
        }

        match self.buf[1] {
            0x00 => {} // succeeded
            0x01 => Err(Error::GeneralSocksServerFailure)?,
            0x02 => Err(Error::ConnectionNotAllowedByRuleset)?,
            0x03 => Err(Error::NetworkUnreachable)?,
            0x04 => Err(Error::HostUnreachable)?,
            0x05 => Err(Error::ConnectionRefused)?,
            0x06 => Err(Error::TtlExpired)?,
            0x07 => Err(Error::CommandNotSupported)?,
            0x08 => Err(Error::AddressTypeNotSupported)?,
            _ => Err(Error::UnknownAuthMethod)?,
        }

        match self.buf[3] {
            // IPv4
            0x01 => {
                self.len = 10;
            }
            // IPv6
            0x04 => {
                self.len = 22;
            }
            // Domain
            0x03 => {
                self.len = 5;
                self.ptr += tcp.read_exact(&mut self.buf[self.ptr..self.len]).await?;
                self.len += self.buf[4] as usize + 2;
            }
            _ => Err(Error::UnknownAddressType)?,
        }

        self.ptr += tcp.read_exact(&mut self.buf[self.ptr..self.len]).await?;
        let target: TargetAddr<'static> = match self.buf[3] {
            // IPv4
            0x01 => {
                let mut ip = [0; 4];
                ip[..].copy_from_slice(&self.buf[4..8]);
                let ip = Ipv4Addr::from(ip);
                let port = u16::from_be_bytes([self.buf[8], self.buf[9]]);
                (ip, port).into_target_addr()?
            }
            // IPv6
            0x04 => {
                let mut ip = [0; 16];
                ip[..].copy_from_slice(&self.buf[4..20]);
                let ip = Ipv6Addr::from(ip);
                let port = u16::from_be_bytes([self.buf[20], self.buf[21]]);
                (ip, port).into_target_addr()?
            }
            // Domain
            0x03 => {
                let domain_bytes = (&self.buf[5..(self.len - 2)]).to_vec();
                let domain = String::from_utf8(domain_bytes)
                    .map_err(|_| Error::InvalidTargetAddress("not a valid UTF-8 string"))?;
                let port = u16::from_be_bytes([self.buf[self.len - 2], self.buf[self.len - 1]]);
                TargetAddr::Domain(domain.into(), port)
            }
            _ => unreachable!(),
        };

        Ok(target)
    }
}

/// A SOCKS5 BIND client.
///
/// Once you get an instance of `Socks5Listener`, you should send the
/// `bind_addr` to the remote process via the primary connection. Then, call the
/// `accept` function and wait for the other end connecting to the rendezvous
/// address.
pub struct Socks5Listener {
    inner: Socks5Stream,
}

impl Socks5Listener {
    /// Initiates a BIND request to the specified proxy.
    ///
    /// The proxy will filter incoming connections based on the value of
    /// `target`.
    ///
    /// # Error
    ///
    /// It propagates the error that occurs in the conversion from `T` to
    /// `TargetAddr`.
    pub async fn bind<'t, P, T>(proxy: P, target: T) -> Result<Socks5Listener>
    where
        P: ToProxyAddrs,
        T: IntoTargetAddr<'t>,
    {
        Self::bind_with_auth(Authentication::None, proxy, target).await
    }

    /// Initiates a BIND request to the specified proxy using given username
    /// and password.
    ///
    /// The proxy will filter incoming connections based on the value of
    /// `target`.
    ///
    /// # Error
    ///
    /// It propagates the error that occurs in the conversion from `T` to
    /// `TargetAddr`.
    pub async fn bind_with_password<'a, 't, P, T>(
        proxy: P,
        target: T,
        username: &'a str,
        password: &'a str,
    ) -> Result<Socks5Listener>
    where
        P: ToProxyAddrs,
        T: IntoTargetAddr<'t>,
    {
        Self::bind_with_auth(Authentication::Password { username, password }, proxy, target).await
    }

    async fn bind_with_auth<'t, P, T>(auth: Authentication<'_>, proxy: P, target: T) -> Result<Socks5Listener>
    where
        P: ToProxyAddrs,
        T: IntoTargetAddr<'t>,
    {
        let socket = SocksConnector::new(
            auth,
            Command::Bind,
            proxy.to_proxy_addrs().fuse(),
            target.into_target_addr()?,
        )
        .execute()
        .await?;

        Ok(Socks5Listener { inner: socket })
    }

    /// Returns the address of the proxy-side TCP listener.
    ///
    /// This should be forwarded to the remote process, which should open a
    /// connection to it.
    pub fn bind_addr(&self) -> TargetAddr {
        self.inner.target_addr()
    }

    /// Consumes this listener, returning a `Future` which resolves to the
    /// `Socks5Stream` connected to the target server through the proxy.
    ///
    /// The value of `bind_addr` should be forwarded to the remote process
    /// before this method is called.
    pub async fn accept(mut self) -> Result<Socks5Stream> {
        let mut connector = SocksConnector {
            auth: Authentication::None,
            command: Command::Bind,
            proxy: stream::empty().fuse(),
            target: self.inner.target,
            buf: [0; 513],
            ptr: 0,
            len: 0,
        };

        let target = connector.receive_reply(&mut self.inner.tcp).await?;

        Ok(Socks5Stream {
            tcp: self.inner.tcp,
            target,
        })
    }
}

impl AsyncRead for Socks5Stream {
    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [std::mem::MaybeUninit<u8>]) -> bool {
        AsyncRead::prepare_uninitialized_buffer(&self.tcp, buf)
    }

    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        AsyncRead::poll_read(Pin::new(&mut self.tcp), cx, buf)
    }
}

impl AsyncWrite for Socks5Stream {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        AsyncWrite::poll_write(Pin::new(&mut self.tcp), cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        AsyncWrite::poll_flush(Pin::new(&mut self.tcp), cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        AsyncWrite::poll_shutdown(Pin::new(&mut self.tcp), cx)
    }
}
