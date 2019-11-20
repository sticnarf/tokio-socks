mod common;

use common::{runtime, test_bind, test_connect, ECHO_SERVER_ADDR, PROXY_ADDR};
use tokio_socks::tcp::Socks5Listener;
use tokio_socks::{tcp::Socks5Stream, Error};

#[test]
fn connect_long_username_password() -> Result<(), Error> {
    let runtime = runtime().lock().unwrap();
    let conn = runtime.block_on(Socks5Stream::connect_with_password(
        PROXY_ADDR, ECHO_SERVER_ADDR, "mylonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglogin",
                                                                    "longlonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglongpassword"))?;
    runtime.block_on(test_connect(conn))
}

#[test]
fn bind_long_username_password() -> Result<(), Error> {
    let bind = {
        let runtime = runtime().lock().unwrap();
        runtime.block_on(Socks5Listener::bind_with_password(
            PROXY_ADDR,
            ECHO_SERVER_ADDR,
            "mylonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglogin",
            "longlonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglonglongpassword"
        ))
    }?;
    test_bind(bind)
}
