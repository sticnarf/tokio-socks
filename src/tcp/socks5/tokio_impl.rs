//! This module contains tokio-specfic implementations.
use super::*;
use crate::ToProxyAddrs;
use tokio::io::ReadBuf;
use tokio::net::TcpStream;

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

impl<T> AsyncRead for Socks5Stream<T>
where
    T: AsyncRead + Unpin,
{
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        AsyncRead::poll_read(Pin::new(&mut self.socket), cx, buf)
    }
}

impl<T> AsyncWrite for Socks5Stream<T>
where
    T: AsyncWrite + Unpin,
{
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        AsyncWrite::poll_write(Pin::new(&mut self.socket), cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        AsyncWrite::poll_flush(Pin::new(&mut self.socket), cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        AsyncWrite::poll_shutdown(Pin::new(&mut self.socket), cx)
    }
}
