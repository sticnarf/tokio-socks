use crate::{Authentication, Error};
use futures::{Future, Poll};
use tokio_tcp::TcpStream;

struct ConnectFuture<'a> {
    auth: Authentication<'a>,
}

struct Socks5Stream {
    socket: TcpStream,
}

impl Socks5Stream {
    fn connect<P, T>(proxy: P, target: T) -> ConnectFuture<'static> {
        unimplemented!()
    }

    fn connect_with_password<'a, P, T>(
        proxy: P,
        target: T,
        username: &'a str,
        password: &'a str,
    ) -> ConnectFuture<'a> {
        unimplemented!()
    }
}

impl<'a> Future for ConnectFuture<'a> {
    type Item = Socks5Stream;
    type Error = Error;

    fn poll(&mut self) -> Poll<Socks5Stream, Error> {
        unimplemented!()
    }
}
