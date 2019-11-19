mod common;

use common::{test_bind, test_connect, ECHO_SERVER_ADDR, PROXY_ADDR};
use futures::future::{err, Future};
use std::{io, net::SocketAddr};
use tokio_socks::{
    tcp::{Socks5Listener, Socks5Stream},
    Error,
};
use tokio_tcp::TcpStream;

type Result<T> = std::result::Result<T, Error>;

#[test]
fn connect() -> Result<()> {
    let conn =
        Socks5Stream::connect_with_password(PROXY_ADDR, ECHO_SERVER_ADDR, "mylonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglogin", "longlonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglongpassword")?;
    test_connect(conn)
}

#[test]
fn bind() -> Result<()> {
    let bind =
        Socks5Listener::bind_with_password(PROXY_ADDR, ECHO_SERVER_ADDR, "mylonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglogin", "longlonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglongpassword")?;
    test_bind(bind)
}

#[test]
fn connect_existing() -> Result<()> {
    let addr = PROXY_ADDR
        .parse::<SocketAddr>()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    let conn = TcpStream::connect(&addr).map_err(Error::Io).and_then(
        |c| -> Box<Future<Item = Socks5Stream, Error = Error> + Send + 'static> {
            match Socks5Stream::connect_existing_with_password(
                c,
                ECHO_SERVER_ADDR,
                "mylonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglogin",
                "longlonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglongpassword",
            ) {
                Ok(f) => Box::new(f),
                Err(e) => Box::new(err(e)),
            }
        },
    );
    test_connect(conn)
}
