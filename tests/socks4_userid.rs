mod common;

use common::*;
use tokio_socks::{tcp::socks4::*, Result};

#[test]
#[cfg(feature = "tokio")]
fn connect_userid() -> Result<()> {
    let runtime = runtime().lock().unwrap();
    let conn = runtime.block_on(Socks4Stream::connect_with_userid(
        SOCKS4_PROXY_ADDR,
        ECHO_SERVER_ADDR,
        "mylogin",
    ))?;
    runtime.block_on(test_connect(conn))
}

#[test]
#[cfg(feature = "tokio")]
fn bind_userid() -> Result<()> {
    let bind = {
        let runtime = runtime().lock().unwrap();
        runtime.block_on(Socks4Listener::bind_with_userid(
            SOCKS4_PROXY_ADDR,
            ECHO_SERVER_ADDR,
            "mylogin",
        ))
    }?;
    test_bind_socks4(bind)
}

#[test]
fn connect_with_socket_userid() -> Result<()> {
    let runtime = runtime().lock().unwrap();
    let socket = runtime.block_on(connect_unix(UNIX_SOCKS4_PROXY_ADDR))?;
    let conn = runtime.block_on(Socks4Stream::connect_with_userid_and_socket(
        socket,
        ECHO_SERVER_ADDR,
        "mylogin",
    ))?;
    runtime.block_on(test_connect(conn))
}

#[test]
fn bind_with_socket_userid() -> Result<()> {
    let bind = {
        let runtime = runtime().lock().unwrap();
        let socket = runtime.block_on(connect_unix(UNIX_SOCKS4_PROXY_ADDR))?;
        runtime.block_on(Socks4Listener::bind_with_user_and_socket(
            socket,
            ECHO_SERVER_ADDR,
            "mylogin",
        ))
    }?;
    test_bind_socks4(bind)
}
