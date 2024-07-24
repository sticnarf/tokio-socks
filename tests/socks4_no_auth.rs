mod common;

use common::*;
use tokio_socks::{
    tcp::socks4::{Socks4Listener, Socks4Stream},
    Result,
};

#[test]
#[cfg(feature = "tokio")]
fn connect_no_auth() -> Result<()> {
    let runtime = runtime().lock().unwrap();
    let conn = runtime.block_on(Socks4Stream::connect(SOCKS4_PROXY_ADDR, ECHO_SERVER_ADDR))?;
    runtime.block_on(test_connect(conn))
}

#[test]
#[cfg(feature = "tokio")]
fn bind_no_auth() -> Result<()> {
    let bind = {
        let runtime = runtime().lock().unwrap();
        runtime.block_on(Socks4Listener::bind(SOCKS4_PROXY_ADDR, ECHO_SERVER_ADDR))
    }?;
    test_bind_socks4(bind)
}

#[test]
fn connect_with_socket_no_auth() -> Result<()> {
    let runtime = runtime().lock().unwrap();
    let socket = runtime.block_on(connect_unix(UNIX_SOCKS4_PROXY_ADDR))?;
    println!("socket connected");
    let conn = runtime.block_on(Socks4Stream::connect_with_socket(socket, ECHO_SERVER_ADDR))?;
    runtime.block_on(test_connect(conn))
}

#[test]
fn bind_with_socket_no_auth() -> Result<()> {
    let bind = {
        let runtime = runtime().lock().unwrap();
        let socket = runtime.block_on(connect_unix(UNIX_SOCKS4_PROXY_ADDR))?;
        runtime.block_on(Socks4Listener::bind_with_socket(socket, ECHO_SERVER_ADDR))
    }?;
    test_bind_socks4(bind)
}
