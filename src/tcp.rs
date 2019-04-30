use crate::{Authentication, Error, IntoTargetAddr, Result, TargetAddr, ToProxyAddrs};
use bytes::{Buf, BufMut};
use derefable::Derefable;
use futures::{stream, try_ready, Async, Future, Poll, Stream};
use std::borrow::Borrow;
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use tokio_io::{AsyncRead, AsyncWrite};
use tokio_tcp::{ConnectFuture as TokioConnect, TcpStream};

#[repr(u8)]
#[derive(Clone, Copy)]
enum Command {
    Connect = 0x01,
    Bind = 0x02,
    Associate = 0x03,
}

/// A SOCKS5 client.
///
/// For convenience, it can be dereferenced to `tokio_tcp::TcpStream`.
#[derive(Debug, Derefable)]
pub struct Socks5Stream {
    #[deref(mutable)]
    tcp: TcpStream,
    target: TargetAddr<'static>,
}

impl Socks5Stream {
    /// Connects to a target server through a SOCKS5 proxy.
    ///
    /// # Error
    ///
    /// It propagates the error that occurs in the conversion from `T` to `TargetAddr`.
    pub fn connect<'t, P, T>(proxy: P, target: T) -> Result<ConnectFuture<'static, 't, P::Output>>
    where
        P: ToProxyAddrs,
        T: IntoTargetAddr<'t>,
    {
        Self::connect_raw(proxy, target, Authentication::None, Command::Connect)
    }

    /// Connects to a target server through a SOCKS5 proxy using given username and password.
    ///
    /// # Error
    ///
    /// It propagates the error that occurs in the conversion from `T` to `TargetAddr`.
    pub fn connect_with_password<'a, 't, P, T>(
        proxy: P,
        target: T,
        username: &'a str,
        password: &'a str,
    ) -> Result<ConnectFuture<'a, 't, P::Output>>
    where
        P: ToProxyAddrs,
        T: IntoTargetAddr<'t>,
    {
        Self::connect_raw(
            proxy,
            target,
            Authentication::Password { username, password },
            Command::Connect,
        )
    }

    fn connect_raw<'a, 't, P, T>(
        proxy: P,
        target: T,
        auth: Authentication<'a>,
        command: Command,
    ) -> Result<ConnectFuture<'a, 't, P::Output>>
    where
        P: ToProxyAddrs,
        T: IntoTargetAddr<'t>,
    {
        if let Authentication::Password { username, password } = auth {
            let username_len = username.as_bytes().len();
            if username_len < 1 || username_len > 255 {
                Err(Error::InvalidAuthValues(
                    "username length should between 1 to 255",
                ))?
            }
            let password_len = password.as_bytes().len();
            if password_len < 1 || password_len > 255 {
                Err(Error::InvalidAuthValues(
                    "password length should between 1 to 255",
                ))?
            }
        }
        Ok(ConnectFuture::new(
            auth,
            command,
            proxy.to_proxy_addrs(),
            target.into_target_addr()?,
        ))
    }

    /// Consumes the `Socks5Stream`, returning the inner `tokio_tcp::TcpStream`.
    pub fn into_inner(self) -> TcpStream {
        self.tcp
    }

    /// Returns the target address that the proxy server connects to.
    pub fn target_addr(&self) -> TargetAddr {
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
pub struct ConnectFuture<'a, 't, S>
where
    S: Stream<Item = SocketAddr, Error = Error>,
{
    auth: Authentication<'a>,
    command: Command,
    proxy: S,
    target: TargetAddr<'t>,
    state: ConnectState,
    buf: [u8; 262],
    ptr: usize,
    len: usize,
}

impl<'a, 't, S> ConnectFuture<'a, 't, S>
where
    S: Stream<Item = SocketAddr, Error = Error>,
{
    fn new(auth: Authentication<'a>, command: Command, proxy: S, target: TargetAddr<'t>) -> Self {
        ConnectFuture {
            auth,
            command,
            proxy,
            target,
            state: ConnectState::Uninitialized,
            buf: [0; 513],
            ptr: 0,
            len: 0,
        }
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
}

impl<'a, 't, S> Future for ConnectFuture<'a, 't, S>
where
    S: Stream<Item = SocketAddr, Error = Error>,
{
    type Item = Socks5Stream;
    type Error = Error;

    fn poll(&mut self) -> Poll<Socks5Stream, Error> {
        loop {
            match self.state {
                ConnectState::Uninitialized => match try_ready!(self.proxy.poll()) {
                    Some(addr) => self.state = ConnectState::Created(TcpStream::connect(&addr)),
                    None => Err(Error::ProxyServerUnreachable)?,
                },
                ConnectState::Created(ref mut conn_fut) => match conn_fut.poll() {
                    Ok(Async::Ready(tcp)) => {
                        self.state = ConnectState::Connected(Some(tcp));
                        self.prepare_send_method_selection()
                    }
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Err(_e) => self.state = ConnectState::Uninitialized,
                },
                ConnectState::Connected(ref mut opt) => {
                    let tcp = opt.as_mut().unwrap();
                    self.ptr += try_ready!(tcp.poll_write(&self.buf[self.ptr..self.len]));
                    ;
                    if self.ptr == self.len {
                        self.state = ConnectState::MethodSent(opt.take());
                        self.prepare_recv_method_selection();
                    }
                }
                ConnectState::MethodSent(ref mut opt) => {
                    let tcp = opt.as_mut().unwrap();
                    self.ptr += try_ready!(tcp.poll_read(&mut self.buf[self.ptr..self.len]));
                    if self.ptr == self.len {
                        if self.buf[0] != 0x05 {
                            Err(Error::InvalidResponseVersion)?
                        }
                        match self.buf[1] {
                            0x00 => self.state = ConnectState::PrepareRequest(opt.take()),
                            0xff => Err(Error::NoAcceptableAuthMethods)?,
                            0x02 => {
                                self.state = ConnectState::PasswordAuth(opt.take());
                                self.prepare_send_password_auth();
                            }
                            m if m != self.auth.id() => Err(Error::UnknownAuthMethod)?,
                            _ => unimplemented!(),
                        }
                    }
                }
                ConnectState::PasswordAuth(ref mut opt) => {
                    let tcp = opt.as_mut().unwrap();
                    self.ptr += try_ready!(tcp.poll_write(&self.buf[self.ptr..self.len]));
                    if self.ptr == self.len {
                        self.state = ConnectState::PasswordAuthSent(opt.take());
                        self.prepare_recv_password_auth();
                    }
                }
                ConnectState::PasswordAuthSent(ref mut opt) => {
                    let tcp = opt.as_mut().unwrap();
                    self.ptr += try_ready!(tcp.poll_read(&mut self.buf[self.ptr..self.len]));
                    if self.ptr == self.len {
                        if self.buf[0] != 0x01 {
                            Err(Error::InvalidResponseVersion)?
                        }
                        if self.buf[1] != 0x00 {
                            Err(Error::PasswordAuthFailure(self.buf[1]))?
                        }
                        self.state = ConnectState::PrepareRequest(opt.take());
                    }
                }
                ConnectState::PrepareRequest(ref mut opt) => {
                    self.state = ConnectState::SendRequest(opt.take());
                    self.prepare_send_request();
                }
                ConnectState::SendRequest(ref mut opt) => {
                    let tcp = opt.as_mut().unwrap();
                    self.ptr += try_ready!(tcp.poll_write(&self.buf[self.ptr..self.len]));
                    if self.ptr == self.len {
                        self.state = ConnectState::RequestSent(opt.take());
                        self.prepare_recv_reply();
                    }
                }
                ConnectState::RequestSent(ref mut opt) => {
                    let tcp = opt.as_mut().unwrap();
                    self.ptr += try_ready!(tcp.poll_read(&mut self.buf[self.ptr..self.len]));
                    if self.ptr == self.len {
                        if self.buf[0] != 0x05 {
                            Err(Error::InvalidResponseVersion)?
                        }
                        if self.buf[2] != 0x00 {
                            Err(Error::InvalidReservedByte)?
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
                                self.state = ConnectState::ReadAddress(opt.take())
                            }
                            // IPv6
                            0x04 => {
                                self.len = 22;
                                self.state = ConnectState::ReadAddress(opt.take())
                            }
                            // Domain
                            0x03 => {
                                self.len = 5;
                                self.state = ConnectState::PrepareReadAddress(opt.take())
                            }
                            _ => Err(Error::UnknownAddressType)?,
                        }
                    }
                }
                ConnectState::PrepareReadAddress(ref mut opt) => {
                    let tcp = opt.as_mut().unwrap();
                    self.ptr += try_ready!(tcp.poll_read(&mut self.buf[self.ptr..self.len]));
                    if self.ptr == self.len {
                        self.len += self.buf[4] as usize + 2;
                        self.state = ConnectState::ReadAddress(opt.take());
                    }
                }
                ConnectState::ReadAddress(ref mut opt) => {
                    let tcp = opt.as_mut().unwrap();
                    self.ptr += try_ready!(tcp.poll_read(&mut self.buf[self.ptr..self.len]));
                    if self.ptr == self.len {
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
                                let domain = String::from_utf8(domain_bytes).map_err(|_| {
                                    Error::InvalidTargetAddress("not a valid UTF-8 string")
                                })?;
                                let port = u16::from_be_bytes([
                                    self.buf[self.len - 2],
                                    self.buf[self.len - 1],
                                ]);
                                TargetAddr::Domain(domain.into(), port)
                            }
                            _ => unreachable!(),
                        };
                        return Ok(Async::Ready(Socks5Stream {
                            tcp: opt.take().unwrap(),
                            target,
                        }));
                    }
                }
            }
        }
    }
}

#[derive(Debug)]
enum ConnectState {
    Uninitialized,
    Created(TokioConnect),
    Connected(Option<TcpStream>),
    MethodSent(Option<TcpStream>),
    PasswordAuth(Option<TcpStream>),
    PasswordAuthSent(Option<TcpStream>),
    PrepareRequest(Option<TcpStream>),
    SendRequest(Option<TcpStream>),
    RequestSent(Option<TcpStream>),
    PrepareReadAddress(Option<TcpStream>),
    ReadAddress(Option<TcpStream>),
}

/// A SOCKS5 BIND client.
///
/// Once you get an instance of `Socks5Listener`, you should send the `bind_addr`
/// to the remote process via the primary connection. Then, call the `accept` function
/// and wait for the other end connecting to the rendezvous address.
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
    /// It propagates the error that occurs in the conversion from `T` to `TargetAddr`.
    pub fn bind<'t, P, T>(proxy: P, target: T) -> Result<BindFuture<'static, 't, P::Output>>
    where
        P: ToProxyAddrs,
        T: IntoTargetAddr<'t>,
    {
        Ok(BindFuture(ConnectFuture::new(
            Authentication::None,
            Command::Bind,
            proxy.to_proxy_addrs(),
            target.into_target_addr()?,
        )))
    }

    /// Initiates a BIND request to the specified proxy using given username
    /// and password.
    ///
    /// The proxy will filter incoming connections based on the value of
    /// `target`.
    ///
    /// # Error
    ///
    /// It propagates the error that occurs in the conversion from `T` to `TargetAddr`.
    pub fn bind_with_password<'a, 't, P, T>(
        proxy: P,
        target: T,
        username: &'a str,
        password: &'a str,
    ) -> Result<BindFuture<'a, 't, P::Output>>
    where
        P: ToProxyAddrs,
        T: IntoTargetAddr<'t>,
    {
        Ok(BindFuture(ConnectFuture::new(
            Authentication::Password { username, password },
            Command::Bind,
            proxy.to_proxy_addrs(),
            target.into_target_addr()?,
        )))
    }

    /// Returns the address of the proxy-side TCP listener.
    ///
    /// This should be forwarded to the remote process, which should open a
    /// connection to it.
    pub fn bind_addr(&self) -> TargetAddr {
        self.inner.target_addr()
    }

    /// Consumes this listener, returning a `Future` which resolves to the `Socks5Stream`
    /// connected to the target server through the proxy.
    ///
    /// The value of `bind_addr` should be forwarded to the remote process
    /// before this method is called.
    pub fn accept(self) -> impl Future<Item = Socks5Stream, Error = Error> {
        let mut conn_fut = ConnectFuture {
            auth: Authentication::None,
            command: Command::Bind,
            proxy: stream::empty(),
            target: self.inner.target,
            state: ConnectState::RequestSent(Some(self.inner.tcp)),
            buf: [0; 262],
            ptr: 0,
            len: 0,
        };
        conn_fut.prepare_recv_reply();
        conn_fut
    }
}

/// A `Future` which resolves to a `Socks5Listener`.
///
/// After this future is resolved, the SOCKS5 client has finished the negotiation
/// with the proxy server.
pub struct BindFuture<'a, 't, S>(ConnectFuture<'a, 't, S>)
where
    S: Stream<Item = SocketAddr, Error = Error>;

impl<'a, 't, S> Future for BindFuture<'a, 't, S>
where
    S: Stream<Item = SocketAddr, Error = Error>,
{
    type Item = Socks5Listener;
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let tcp = try_ready!(self.0.poll());
        Ok(Async::Ready(Socks5Listener { inner: tcp }))
    }
}

impl Read for Socks5Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.tcp.read(buf)
    }
}

impl Write for Socks5Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.tcp.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.tcp.flush()
    }
}

impl AsyncRead for Socks5Stream {
    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [u8]) -> bool {
        self.tcp.prepare_uninitialized_buffer(buf)
    }

    fn read_buf<B: BufMut>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        self.tcp.read_buf(buf)
    }
}

impl AsyncWrite for Socks5Stream {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        AsyncWrite::shutdown(&mut self.tcp)
    }

    fn write_buf<B: Buf>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        self.tcp.write_buf(buf)
    }
}

impl<'a> Read for &'a Socks5Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        Read::read(&mut &self.tcp, buf)
    }
}

impl<'a> Write for &'a Socks5Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        Write::write(&mut &self.tcp, buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        Write::flush(&mut &self.tcp)
    }
}

impl<'a> AsyncRead for &'a Socks5Stream {
    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [u8]) -> bool {
        AsyncRead::prepare_uninitialized_buffer(&self.tcp, buf)
    }

    fn read_buf<B: BufMut>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        AsyncRead::read_buf(&mut &self.tcp, buf)
    }
}

impl<'a> AsyncWrite for &'a Socks5Stream {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        AsyncWrite::shutdown(&mut &self.tcp)
    }

    fn write_buf<B: Buf>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        AsyncWrite::write_buf(&mut &self.tcp, buf)
    }
}
