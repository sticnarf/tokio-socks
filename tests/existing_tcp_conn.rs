mod common;

use common::{test_connect, ECHO_SERVER_ADDR, PROXY_ADDR};
use futures::future::{err, Future};
use std::io;
use std::net::SocketAddr;
use tokio_socks::{tcp::Socks5Stream, Error};
use tokio_tcp::TcpStream;

type Result<T> = std::result::Result<T, Error>;

#[test]
fn connect() -> Result<()> {
    let addr = PROXY_ADDR
        .parse::<SocketAddr>()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    let conn = TcpStream::connect(&addr).map_err(Error::Io).and_then(
        |c| -> Box<Future<Item = Socks5Stream, Error = Error> + Send + 'static> {
            match Socks5Stream::handshake(c, ECHO_SERVER_ADDR) {
                Ok(f) => Box::new(f),
                Err(e) => Box::new(err(e)),
            }
        },
    );
    test_connect(conn)
}
