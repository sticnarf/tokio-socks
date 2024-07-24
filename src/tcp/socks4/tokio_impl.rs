//! This module contains tokio-specfic implementations.
use super::*;
use crate::ToProxyAddrs;
use tokio::io::ReadBuf;
use tokio::net::TcpStream;

impl Socks4Stream<TcpStream> {
    /// Connects to a target server through a SOCKS4 proxy given the proxy
    /// address.
    ///
    /// # Error
    ///
    /// It propagates the error that occurs in the conversion from `T` to
    /// `TargetAddr`.
    pub async fn connect<'t, P, T>(proxy: P, target: T) -> Result<Socks4Stream<TcpStream>>
    where
        P: ToProxyAddrs,
        T: IntoTargetAddr<'t>,
    {
        Self::execute_command(proxy, target, None, CommandV4::Connect).await
    }

    /// Connects to a target server through a SOCKS4 proxy using given username,
    /// password and the address of the proxy.
    ///
    /// # Error
    ///
    /// It propagates the error that occurs in the conversion from `T` to
    /// `TargetAddr`.
    pub async fn connect_with_userid<'a, 't, P, T>(
        proxy: P,
        target: T,
        user_id: &'a str,
    ) -> Result<Socks4Stream<TcpStream>>
    where
        P: ToProxyAddrs,
        T: IntoTargetAddr<'t>,
    {
        Self::execute_command(proxy, target, Some(user_id), CommandV4::Connect).await
    }

    async fn execute_command<'a, 't, P, T>(
        proxy: P,
        target: T,
        user_id: Option<&'a str>,
        command: CommandV4,
    ) -> Result<Socks4Stream<TcpStream>>
    where
        P: ToProxyAddrs,
        T: IntoTargetAddr<'t>,
    {
        Self::validate_userid(user_id)?;

        let sock = Socks4Connector::new(
            user_id,
            command,
            proxy.to_proxy_addrs().fuse(),
            target.into_target_addr()?,
        )
        .execute()
        .await?;

        Ok(sock)
    }
}

impl Socks4Listener<TcpStream> {
    /// Initiates a BIND request to the specified proxy.
    ///
    /// The proxy will filter incoming connections based on the value of
    /// `target`.
    ///
    /// # Error
    ///
    /// It propagates the error that occurs in the conversion from `T` to
    /// `TargetAddr`.
    pub async fn bind<'t, P, T>(proxy: P, target: T) -> Result<Socks4Listener<TcpStream>>
    where
        P: ToProxyAddrs,
        T: IntoTargetAddr<'t>,
    {
        Self::bind_to_target(None, proxy, target).await
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
    pub async fn bind_with_userid<'a, 't, P, T>(
        proxy: P,
        target: T,
        user_id: &'a str,
    ) -> Result<Socks4Listener<TcpStream>>
    where
        P: ToProxyAddrs,
        T: IntoTargetAddr<'t>,
    {
        Self::bind_to_target(Some(user_id), proxy, target).await
    }

    async fn bind_to_target<'a, 't, P, T>(
        user_id: Option<&'a str>,
        proxy: P,
        target: T,
    ) -> Result<Socks4Listener<TcpStream>>
    where
        P: ToProxyAddrs,
        T: IntoTargetAddr<'t>,
    {
        let socket = Socks4Connector::new(
            user_id,
            CommandV4::Bind,
            proxy.to_proxy_addrs().fuse(),
            target.into_target_addr()?,
        )
        .execute()
        .await?;

        Ok(Socks4Listener { inner: socket })
    }
}

impl<T> AsyncRead for Socks4Stream<T>
where
    T: AsyncRead + Unpin,
{
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        AsyncRead::poll_read(Pin::new(&mut self.socket), cx, buf)
    }
}

impl<T> AsyncWrite for Socks4Stream<T>
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
