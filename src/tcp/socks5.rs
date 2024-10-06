#[cfg(feature = "gssapi")]
use crate::GssapiAuthenticator;
use std::{
    borrow::Borrow,
    io,
    net::{Ipv4Addr, Ipv6Addr, SocketAddr},
    ops::{Deref, DerefMut},
    pin::Pin,
    task::{Context, Poll},
};

use futures_util::stream::{self, Fuse, Stream, StreamExt};
#[cfg(feature = "tokio")]
use tokio::net::TcpStream;

#[cfg(feature = "tokio")]
use crate::ToProxyAddrs;
use crate::{
    io::{AsyncSocket, AsyncSocketExt},
    Authentication, Error, IntoTargetAddr, Result, TargetAddr,
};

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
/// For convenience, it can be dereferenced to it's inner socket.
#[derive(Debug)]
pub struct Socks5Stream<S> {
    socket: S,
    target: TargetAddr<'static>,
}

impl<S> Deref for Socks5Stream<S> {
    type Target = S;

    fn deref(&self) -> &Self::Target {
        &self.socket
    }
}

impl<S> DerefMut for Socks5Stream<S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.socket
    }
}

#[cfg(feature = "tokio")]
impl Socks5Stream<TcpStream> {
    /// Connects to a target server through a SOCKS5 proxy given the proxy
    /// address.
    ///
    /// # Error
    ///
    /// It propagates the error that occurs in the conversion from `T` to
    /// `TargetAddr`.
    pub async fn connect<'t, P, T>(proxy: P, target: T) -> Result<Socks5Stream<TcpStream>>
    where
        P: ToProxyAddrs,
        T: IntoTargetAddr<'t>,
    {
        Self::execute_command(proxy, target, Authentication::None, Command::Connect).await
    }

    /// Connects to a target server through a SOCKS5 proxy given the proxy
    /// address and authenticates via gssapi.
    ///
    /// # Error
    ///
    /// It propagates the error that occurs in the conversion from `T` to
    /// `TargetAddr`.
    #[cfg(feature = "gssapi")]
    pub async fn connect_with_gssapi<'a, 't, P, T>(
        proxy: P,
        target: T,
        gssapi_authenticator: GssapiAuthenticator<'a>,
    ) -> Result<Socks5Stream<TcpStream>>
    where
        P: ToProxyAddrs,
        T: IntoTargetAddr<'t>,
    {
        Self::execute_command(
            proxy,
            target,
            Authentication::Gssapi { gssapi_authenticator },
            Command::Connect,
        )
        .await
    }

    /// Connects to a target server through a SOCKS5 proxy using given username,
    /// password and the address of the proxy.
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
    ) -> Result<Socks5Stream<TcpStream>>
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

    #[cfg(feature = "tor")]
    /// Resolve the domain name to an ip using special Tor Resolve command, by
    /// connecting to a Tor compatible proxy given it's address.
    pub async fn tor_resolve<'t, P, T>(proxy: P, target: T) -> Result<TargetAddr<'static>>
    where
        P: ToProxyAddrs,
        T: IntoTargetAddr<'t>,
    {
        let sock = Self::execute_command(proxy, target, Authentication::None, Command::TorResolve).await?;

        Ok(sock.target_addr().to_owned())
    }

    #[cfg(feature = "tor")]
    /// Perform a reverse DNS query on the given ip using special Tor Resolve
    /// PTR command, by connecting to a Tor compatible proxy given it's
    /// address.
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
    ) -> Result<Socks5Stream<TcpStream>>
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
}

impl<S> Socks5Stream<S>
where
    S: AsyncSocket + Unpin,
{
    /// Connects to a target server through a SOCKS5 proxy given a socket to it.
    ///
    /// # Error
    ///
    /// It propagates the error that occurs in the conversion from `T` to
    /// `TargetAddr`.
    pub async fn connect_with_socket<'t, T>(socket: S, target: T) -> Result<Socks5Stream<S>>
    where
        T: IntoTargetAddr<'t>,
    {
        Self::execute_command_with_socket(socket, target, Authentication::None, Command::Connect).await
    }

    /// Connects to a target server through a SOCKS5 proxy given the proxy
    /// address and authenticates via gssapi.
    ///
    /// # Error
    ///
    /// It propagates the error that occurs in the conversion from `T` to
    /// `TargetAddr`.
    #[cfg(feature = "gssapi")]
    pub async fn connect_with_gssapi_and_socket<'a, 't, T>(
        socket: S,
        target: T,
        gssapi_authenticator: GssapiAuthenticator<'a>,
    ) -> Result<Socks5Stream<S>>
    where
        T: IntoTargetAddr<'t>,
    {
        Self::execute_command_with_socket(
            socket,
            target,
            Authentication::Gssapi { gssapi_authenticator },
            Command::Connect,
        )
        .await
    }

    /// Connects to a target server through a SOCKS5 proxy using given username,
    /// password and a socket to the proxy
    ///
    /// # Error
    ///
    /// It propagates the error that occurs in the conversion from `T` to
    /// `TargetAddr`.
    pub async fn connect_with_password_and_socket<'a, 't, T>(
        socket: S,
        target: T,
        username: &'a str,
        password: &'a str,
    ) -> Result<Socks5Stream<S>>
    where
        T: IntoTargetAddr<'t>,
    {
        Self::execute_command_with_socket(
            socket,
            target,
            Authentication::Password { username, password },
            Command::Connect,
        )
        .await
    }

    fn validate_auth(auth: &Authentication<'_>) -> Result<()> {
        match auth {
            Authentication::Password { username, password } => {
                let username_len = username.as_bytes().len();
                if !(1..=255).contains(&username_len) {
                    Err(Error::InvalidAuthValues("username length should between 1 to 255"))?
                }
                let password_len = password.as_bytes().len();
                if !(1..=255).contains(&password_len) {
                    Err(Error::InvalidAuthValues("password length should between 1 to 255"))?
                }
            },
            Authentication::None => {},
            #[cfg(feature = "gssapi")]
            Authentication::Gssapi { .. } => {},
        }
        Ok(())
    }

    #[cfg(feature = "tor")]
    /// Resolve the domain name to an ip using special Tor Resolve command, by
    /// connecting to a Tor compatible proxy given a socket to it.
    pub async fn tor_resolve_with_socket<'t, T>(socket: S, target: T) -> Result<TargetAddr<'static>>
    where
        T: IntoTargetAddr<'t>,
    {
        let sock = Self::execute_command_with_socket(socket, target, Authentication::None, Command::TorResolve).await?;

        Ok(sock.target_addr().to_owned())
    }

    #[cfg(feature = "tor")]
    /// Perform a reverse DNS query on the given ip using special Tor Resolve
    /// PTR command, by connecting to a Tor compatible proxy given a socket
    /// to it.
    pub async fn tor_resolve_ptr_with_socket<'t, T>(socket: S, target: T) -> Result<TargetAddr<'static>>
    where
        T: IntoTargetAddr<'t>,
    {
        let sock =
            Self::execute_command_with_socket(socket, target, Authentication::None, Command::TorResolvePtr).await?;

        Ok(sock.target_addr().to_owned())
    }

    async fn execute_command_with_socket<'a, 't, T>(
        socket: S,
        target: T,
        auth: Authentication<'a>,
        command: Command,
    ) -> Result<Socks5Stream<S>>
    where
        T: IntoTargetAddr<'t>,
    {
        Self::validate_auth(&auth)?;

        let sock = SocksConnector::new(auth, command, stream::empty().fuse(), target.into_target_addr()?)
            .execute_with_socket(socket)
            .await?;

        Ok(sock)
    }

    /// Consumes the `Socks5Stream`, returning the inner socket.
    pub fn into_inner(self) -> S {
        self.socket
    }

    /// Returns the target address that the proxy server connects to.
    pub fn target_addr(&self) -> TargetAddr<'_> {
        match &self.target {
            TargetAddr::Ip(addr) => TargetAddr::Ip(*addr),
            TargetAddr::Domain(domain, port) => {
                let domain: &str = domain.borrow();
                TargetAddr::Domain(domain.into(), *port)
            },
        }
    }
}

/// A `Future` which resolves to a socket to the target server through proxy.
pub struct SocksConnector<'a, 't, S> {
    auth: Authentication<'a>,
    command: Command,
    #[allow(dead_code)]
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

    #[cfg(feature = "tokio")]
    /// Connect to the proxy server, authenticate and issue the SOCKS command
    pub async fn execute(&mut self) -> Result<Socks5Stream<TcpStream>> {
        let next_addr = self.proxy.select_next_some().await?;
        let tcp = TcpStream::connect(next_addr)
            .await
            .map_err(|_| Error::ProxyServerUnreachable)?;

        self.execute_with_socket(tcp).await
    }

    pub async fn execute_with_socket<T: AsyncSocket + Unpin>(&mut self, mut socket: T) -> Result<Socks5Stream<T>> {
        self.authenticate(&mut socket).await?;

        // Send request address that should be proxied
        self.prepare_send_request();
        socket.write_all(&self.buf[self.ptr..self.len]).await?;

        let target = self.receive_reply(&mut socket).await?;

        Ok(Socks5Stream { socket, target })
    }

    fn prepare_send_method_selection(&mut self) {
        self.ptr = 0;
        self.buf[0] = 0x05;
        match self.auth {
            Authentication::None => {
                self.buf[1..3].copy_from_slice(&[1, 0x00]);
                self.len = 3;
            },
            #[cfg(feature = "gssapi")]
            Authentication::Gssapi { .. } => {
                self.buf[1..4].copy_from_slice(&[2, 0x00, 0x01]);
                self.len = 4;
            },
            Authentication::Password { .. } => {
                self.buf[1..4].copy_from_slice(&[2, 0x00, 0x02]);
                self.len = 4;
            },
        }
    }

    fn prepare_recv_method_selection(&mut self) {
        self.ptr = 0;
        self.len = 2;
    }

    #[cfg(feature = "gssapi")]
    async fn prepare_send_gssapi_subnego_token(&mut self) -> Result<Vec<u8>> {
        if let Authentication::Gssapi { gssapi_authenticator } = &self.auth {
            /*
                https://www.rfc-editor.org/rfc/rfc1961

                The security context protection level is sent from client to server
                and vice versa using the following protected message format:

                +------+------+------+.......................+
                + ver  | mtyp | len  |   token               |
                +------+------+------+.......................+
                + 0x01 | 0x02 | 0x02 | up to 2^16 - 1 octets |
                +------+------+------+.......................+

                Where:
                    - "ver" is the protocol version number, here 1 to represent the first version of the SOCKS/GSS-API protocol

                    - "mtyp" is the message type, here 2 to represent a protection-level negotiation message

                    - "len" is the length of the "token" field in octets

                    - "token" is the GSS-API encapsulated protection level
            */
            let mut gssapi_buf: Vec<u8> = Vec::with_capacity(512);
            gssapi_buf.push(0x01); // ver
            gssapi_buf.push(0x02); // mtyp
            let snego_token = gssapi_authenticator.gssapi_authenticator.get_protection_level().await?;
            let snego_token_len = u16::to_be_bytes(snego_token.len() as u16);
            gssapi_buf.extend_from_slice(&snego_token_len); // len
            gssapi_buf.extend(snego_token); // token
            Ok(gssapi_buf)
        } else {
            unreachable!()
        }
    }

    #[cfg(feature = "gssapi")]
    async fn prepare_send_gssapi_sec_context(&mut self, renegotiate_token: Option<&[u8]>) -> Result<Vec<u8>> {
        if let Authentication::Gssapi { gssapi_authenticator } = &self.auth {
            /*
                https://www.rfc-editor.org/rfc/rfc1961

                The client's GSS-API implementation then typically responds with the
                resulting output_token which the client sends in a message to the
                server.

                +------+------+------+.......................+
                + ver  | mtyp | len  |       token           |
                +------+------+------+.......................+
                + 0x01 | 0x01 | 0x02 | up to 2^16 - 1 octets |
                +------+------+------+.......................+

                Where:

                - "ver" is the protocol version number, here 1 to represent the
                    first version of the SOCKS/GSS-API protocol

                - "mtyp" is the message type, here 1 to represent an
                    authentication message

                - "len" is the length of the "token" field in octets

                - "token" is the opaque authentication token emitted by GSS-API
            */
            let mut gssapi_buf: Vec<u8> = Vec::with_capacity(512);
            gssapi_buf.push(0x01); // ver
            gssapi_buf.push(0x01); // mtyp

            let context_token = gssapi_authenticator
                .gssapi_authenticator
                .get_security_context(renegotiate_token)
                .await?;
            let context_token_len = u16::to_be_bytes(context_token.len() as u16);
            gssapi_buf.extend_from_slice(&context_token_len); // len
            gssapi_buf.extend(context_token); // token
            Ok(gssapi_buf)
        } else {
            unreachable!()
        }
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
            self.buf[2 + username_len] = password_len as u8;
            self.buf[(3 + username_len)..self.len].copy_from_slice(password_bytes);
        } else {
            unreachable!()
        }
    }

    #[cfg(feature = "gssapi")]
    fn prepare_recv_gssapi_auth(&mut self) {
        self.ptr = 0;
        self.len = 2;
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
            },
            TargetAddr::Ip(SocketAddr::V6(addr)) => {
                self.buf[3] = 0x04;
                self.buf[4..20].copy_from_slice(&addr.ip().octets());
                self.buf[20..22].copy_from_slice(&addr.port().to_be_bytes());
                self.len = 22;
            },
            TargetAddr::Domain(domain, port) => {
                self.buf[3] = 0x03;
                let domain = domain.as_bytes();
                let len = domain.len();
                self.buf[4] = len as u8;
                self.buf[5..5 + len].copy_from_slice(domain);
                self.buf[(5 + len)..(7 + len)].copy_from_slice(&port.to_be_bytes());
                self.len = 7 + len;
            },
        }
    }

    fn prepare_recv_reply(&mut self) {
        self.ptr = 0;
        self.len = 4;
    }

    #[cfg(feature = "gssapi")]
    async fn gssapi_authentication_protocol<T: AsyncSocket + Unpin>(&mut self, tcp: &mut T) -> Result<()> {
        // Implement Gssapi Auth Protocol.
        // Error out if: Server selected gssapi but, we had None
        let renegotiate_sec_token = match &self.auth {
            Authentication::Gssapi { gssapi_authenticator } => gssapi_authenticator.renegotiate_sec_token,
            _ => return Err(Error::InvalidAuthValues("Server expected GSSApi auth")),
        };

        // Send sec_context token for first time with no renegotiation.
        let sec_context_buf = self.prepare_send_gssapi_sec_context(None).await?;
        tcp.write_all(&sec_context_buf).await?;

        // Recieve and Validate server response
        self.prepare_recv_gssapi_auth();
        tcp.read_exact(&mut self.buf[self.ptr..self.len]).await?;

        if self.buf[1] == 0xff {
            /*
                If the server refuses the client's connection for any reason (GSS-API authentication failure or otherwise), it will return:
                    +------+------+
                    + ver  | mtyp |
                    +------+------+
                    + 0x01 | 0xff |
                    +------+------+

                Where:

                - "ver" is the protocol version number, here 1 to represent the
                first version of the SOCKS/GSS-API protocol

                - "mtyp" is the message type, here 0xff to represent an abort
                message
            */
            return Err(Error::GssapiAuthFailure(self.buf[1]));
        } else {
            /*
                In all continue/confirmation cases, the server uses the same message
                type as for the client -> server interaction.

                +------+------+------+.......................+
                + ver  | mtyp | len  |       token           |
                +------+------+------+.......................+
                + 0x01 | 0x01 | 0x02 | up to 2^16 - 1 octets |
                +------+------+------+.......................+
            */

            // On sec_context validation done.
            // 1.a. Get length of output_token from response. If this token is non-empty we need to renegotiate.
            self.prepare_recv_gssapi_auth();
            tcp.read_exact(&mut self.buf[self.ptr..self.len]).await?;

            let renego_challenge_len = u16::from_be_bytes([self.buf[0], self.buf[1]]);
            // If the sub_negotiation challenge is non-empty get the challenge returned and renegotiate.
            if renego_challenge_len > 0 {
                /*
                    If gss_init_sec_context returns GSS_S_CONTINUE_NEEDED, then the
                    client should expect the server to issue a token in the
                    subsequent subnegotiation response.  The client must pass the
                    token to another call to gss_init_sec_context, and repeat this
                    procedure until "continue" operations are complete.
                */
                // Currently supporting only single re-negotitation.

                let mut renego_challenge: Vec<u8> = Vec::with_capacity(renego_challenge_len as usize);
                tcp.read_exact(&mut renego_challenge).await?;

                // Do renegotiation only if user has specified to do so.
                if renegotiate_sec_token {
                    let sec_context_buf = self.prepare_send_gssapi_sec_context(Some(&renego_challenge)).await?;
                    tcp.write_all(&sec_context_buf).await?;

                    // Check for success of renegotiation
                    self.prepare_recv_gssapi_auth();
                    tcp.read_exact(&mut self.buf[self.ptr..self.len]).await?;

                    if self.buf[1] == 0xff {
                        return Err(Error::GssapiAuthFailure(0));
                    } else {
                        self.prepare_recv_gssapi_auth();
                        tcp.read_exact(&mut self.buf[self.ptr..self.len]).await?;

                        let renego_challenge_len = u16::from_be_bytes([self.buf[0], self.buf[1]]);
                        // drain stream of the renegotiate token if any
                        let mut renego_challenge: Vec<u8> = Vec::with_capacity(renego_challenge_len as usize);
                        tcp.read_exact(&mut renego_challenge).await?;
                        // assume negotiation has succeded.
                    }
                }
            }

            let gssapi_buf = self.prepare_send_gssapi_subnego_token().await?;
            tcp.write_all(&gssapi_buf).await?;

            // recv response
            self.prepare_recv_gssapi_auth();
            tcp.read_exact(&mut self.buf[self.ptr..self.len]).await?;

            if self.buf[1] == 0x02 {
                // Subnegotiation was success.
                // If there is anything sent by server. We can drain the remaining buf, as we do not need it.
                self.prepare_recv_gssapi_auth();
                tcp.read_exact(&mut self.buf[self.ptr..self.len]).await?;
                let remainder_len = u16::from_be_bytes([self.buf[0], self.buf[1]]);

                let mut remainder_buf: Vec<u8> = Vec::with_capacity(remainder_len as usize);
                tcp.read_exact(&mut remainder_buf).await?;
            } else {
                return Err(Error::GssapiAuthFailure(self.buf[1]));
            }
        }
        Ok(())
    }

    async fn password_authentication_protocol<T: AsyncSocket + Unpin>(&mut self, tcp: &mut T) -> Result<()> {
        if let Authentication::None = self.auth {
            return Err(Error::AuthorizationRequired);
        }

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

    async fn authenticate<T: AsyncSocket + Unpin>(&mut self, tcp: &mut T) -> Result<()> {
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
            },
            0x02 => {
                self.password_authentication_protocol(tcp).await?;
            },
            #[cfg(feature = "gssapi")]
            0x01 => {
                // Gssapi Auth Selected -- currently only `required per-message integrity -- 0x01` supported is added for subnegotiation
                self.gssapi_authentication_protocol(tcp).await?;
            },
            0xff => {
                return Err(Error::NoAcceptableAuthMethods);
            },
            m if m != self.auth.id() => return Err(Error::UnknownAuthMethod),
            _ => unimplemented!(),
        }

        Ok(())
    }

    async fn receive_reply<T: AsyncSocket + Unpin>(&mut self, tcp: &mut T) -> Result<TargetAddr<'static>> {
        self.prepare_recv_reply();
        self.ptr += tcp.read_exact(&mut self.buf[self.ptr..self.len]).await?;
        if self.buf[0] != 0x05 {
            return Err(Error::InvalidResponseVersion);
        }
        if self.buf[2] != 0x00 {
            return Err(Error::InvalidReservedByte);
        }

        match self.buf[1] {
            0x00 => {}, // succeeded
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
            },
            // IPv6
            0x04 => {
                self.len = 22;
            },
            // Domain
            0x03 => {
                self.len = 5;
                self.ptr += tcp.read_exact(&mut self.buf[self.ptr..self.len]).await?;
                self.len += self.buf[4] as usize + 2;
            },
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
            },
            // IPv6
            0x04 => {
                let mut ip = [0; 16];
                ip[..].copy_from_slice(&self.buf[4..20]);
                let ip = Ipv6Addr::from(ip);
                let port = u16::from_be_bytes([self.buf[20], self.buf[21]]);
                (ip, port).into_target_addr()?
            },
            // Domain
            0x03 => {
                let domain_bytes = self.buf[5..(self.len - 2)].to_vec();
                let domain = String::from_utf8(domain_bytes)
                    .map_err(|_| Error::InvalidTargetAddress("not a valid UTF-8 string"))?;
                let port = u16::from_be_bytes([self.buf[self.len - 2], self.buf[self.len - 1]]);
                TargetAddr::Domain(domain.into(), port)
            },
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
pub struct Socks5Listener<S> {
    inner: Socks5Stream<S>,
}

#[cfg(feature = "tokio")]
impl Socks5Listener<TcpStream> {
    /// Initiates a BIND request to the specified proxy.
    ///
    /// The proxy will filter incoming connections based on the value of
    /// `target`.
    ///
    /// # Error
    ///
    /// It propagates the error that occurs in the conversion from `T` to
    /// `TargetAddr`.
    pub async fn bind<'t, P, T>(proxy: P, target: T) -> Result<Socks5Listener<TcpStream>>
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
    ) -> Result<Socks5Listener<TcpStream>>
    where
        P: ToProxyAddrs,
        T: IntoTargetAddr<'t>,
    {
        Self::bind_with_auth(Authentication::Password { username, password }, proxy, target).await
    }

    async fn bind_with_auth<'t, P, T>(
        auth: Authentication<'_>,
        proxy: P,
        target: T,
    ) -> Result<Socks5Listener<TcpStream>>
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
}

impl<S> Socks5Listener<S>
where
    S: AsyncSocket + Unpin,
{
    /// Initiates a BIND request to the specified proxy using the given socket
    /// to it.
    ///
    /// The proxy will filter incoming connections based on the value of
    /// `target`.
    ///
    /// # Error
    ///
    /// It propagates the error that occurs in the conversion from `T` to
    /// `TargetAddr`.
    pub async fn bind_with_socket<'t, T>(socket: S, target: T) -> Result<Socks5Listener<S>>
    where
        T: IntoTargetAddr<'t>,
    {
        Self::bind_with_auth_and_socket(Authentication::None, socket, target).await
    }

    /// Initiates a BIND request to the specified proxy using given username,
    /// password and socket to the proxy.
    ///
    /// The proxy will filter incoming connections based on the value of
    /// `target`.
    ///
    /// # Error
    ///
    /// It propagates the error that occurs in the conversion from `T` to
    /// `TargetAddr`.
    pub async fn bind_with_password_and_socket<'a, 't, T>(
        socket: S,
        target: T,
        username: &'a str,
        password: &'a str,
    ) -> Result<Socks5Listener<S>>
    where
        T: IntoTargetAddr<'t>,
    {
        Self::bind_with_auth_and_socket(Authentication::Password { username, password }, socket, target).await
    }

    async fn bind_with_auth_and_socket<'t, T>(
        auth: Authentication<'_>,
        socket: S,
        target: T,
    ) -> Result<Socks5Listener<S>>
    where
        T: IntoTargetAddr<'t>,
    {
        let socket = SocksConnector::new(auth, Command::Bind, stream::empty().fuse(), target.into_target_addr()?)
            .execute_with_socket(socket)
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
    pub async fn accept(mut self) -> Result<Socks5Stream<S>> {
        let mut connector = SocksConnector {
            auth: Authentication::None,
            command: Command::Bind,
            proxy: stream::empty().fuse(),
            target: self.inner.target,
            buf: [0; 513],
            ptr: 0,
            len: 0,
        };

        let target = connector.receive_reply(&mut self.inner.socket).await?;

        Ok(Socks5Stream {
            socket: self.inner.socket,
            target,
        })
    }
}

#[cfg(feature = "tokio")]
impl<T> tokio::io::AsyncRead for Socks5Stream<T>
where
    T: tokio::io::AsyncRead + Unpin,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        tokio::io::AsyncRead::poll_read(Pin::new(&mut self.socket), cx, buf)
    }
}

#[cfg(feature = "tokio")]
impl<T> tokio::io::AsyncWrite for Socks5Stream<T>
where
    T: tokio::io::AsyncWrite + Unpin,
{
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        tokio::io::AsyncWrite::poll_write(Pin::new(&mut self.socket), cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        tokio::io::AsyncWrite::poll_flush(Pin::new(&mut self.socket), cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        tokio::io::AsyncWrite::poll_shutdown(Pin::new(&mut self.socket), cx)
    }
}

#[cfg(feature = "futures-io")]
impl<T> futures_io::AsyncRead for Socks5Stream<T>
where
    T: futures_io::AsyncRead + Unpin,
{
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        futures_io::AsyncRead::poll_read(Pin::new(&mut self.socket), cx, buf)
    }
}

#[cfg(feature = "futures-io")]
impl<T> futures_io::AsyncWrite for Socks5Stream<T>
where
    T: futures_io::AsyncWrite + Unpin,
{
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        futures_io::AsyncWrite::poll_write(Pin::new(&mut self.socket), cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        futures_io::AsyncWrite::poll_flush(Pin::new(&mut self.socket), cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        futures_io::AsyncWrite::poll_close(Pin::new(&mut self.socket), cx)
    }
}
