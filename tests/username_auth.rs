mod common;

use common::runtime;
use tokio::{
    io::{read_exact, write_all},
    prelude::*,
};
use tokio_socks::{tcp::Socks5Stream, Error};

type Result<T> = std::result::Result<T, Error>;

#[test]
fn connect() -> Result<()> {
    const MSG: &[u8] = b"hello";
    let conn = Socks5Stream::connect_with_password(
        "127.0.0.1:41080",
        "localhost:10007",
        "mylogin",
        "mypassword",
    )?;
    let fut = conn
        .and_then(|tcp| write_all(tcp, MSG).map_err(Into::into))
        .and_then(|(tcp, _)| read_exact(tcp, [0; 5]).map_err(Into::into))
        .map(|(_, v)| v);

    let runtime = runtime();
    let res = runtime.lock().unwrap().block_on(fut)?;
    assert_eq!(&res[..], MSG);
    Ok(())
}
