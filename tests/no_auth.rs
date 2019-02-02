mod common;

use common::{test_bind, test_connect, ECHO_SERVER_ADDR, PROXY_ADDR};
use tokio_socks::{
    tcp::{Socks5Listener, Socks5Stream},
    Error,
};

type Result<T> = std::result::Result<T, Error>;

#[test]
fn connect() -> Result<()> {
    let conn = Socks5Stream::connect(PROXY_ADDR, ECHO_SERVER_ADDR)?;
    test_connect(conn)
}

#[test]
fn bind() -> Result<()> {
    let bind = Socks5Listener::bind(PROXY_ADDR, ECHO_SERVER_ADDR)?;
    test_bind(bind)
}
