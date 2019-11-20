mod common;

use crate::common::{runtime, test_bind};
use common::{test_connect, ECHO_SERVER_ADDR, PROXY_ADDR};
use tokio_socks::tcp::Socks5Listener;
use tokio_socks::{tcp::Socks5Stream, Error};

#[test]
fn connect_no_auth() -> Result<(), Error> {
    let runtime = runtime().lock().unwrap();
    let conn = runtime.block_on(Socks5Stream::connect(PROXY_ADDR, ECHO_SERVER_ADDR))?;
    runtime.block_on(test_connect(conn))
}

#[test]
fn bind_no_auth() -> Result<(), Error> {
    let bind = {
        let runtime = runtime().lock().unwrap();
        runtime.block_on(Socks5Listener::bind(PROXY_ADDR, ECHO_SERVER_ADDR))
    }?;
    test_bind(bind)
}
