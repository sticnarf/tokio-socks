use crate::{Authentication, Error, Result, TargetAddr, ToProxyAddrs, ToTargetAddr};
use futures::{Future, Poll, Stream};
use std::net::SocketAddr;
use tokio_tcp::TcpStream;

pub struct ConnectFuture<'a, 't, S>
where
    S: Stream<Item = SocketAddr, Error = Error>,
{
    auth: Authentication<'a>,
    proxy: S,
    target: TargetAddr<'t>,
}

pub struct Socks5Stream {
    socket: TcpStream,
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
        T: ToTargetAddr<'t>,
    {
        Ok(ConnectFuture {
            auth: Authentication::None,
            proxy: proxy.to_proxy_addrs(),
            target: target.to_target_addr()?,
        })
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
        T: ToTargetAddr<'t>,
    {
        Ok(ConnectFuture {
            auth: Authentication::Password { username, password },
            proxy: proxy.to_proxy_addrs(),
            target: target.to_target_addr()?,
        })
    }
}

impl<'a, 't, S> Future for ConnectFuture<'a, 't, S>
where
    S: Stream<Item = SocketAddr, Error = Error>,
{
    type Item = Socks5Stream;
    type Error = Error;

    fn poll(&mut self) -> Poll<Socks5Stream, Error> {
        unimplemented!()
    }
}
