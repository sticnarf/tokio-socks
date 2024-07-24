//! Test the tor proxy capabilities
//!
//! This example requires a running tor proxy.
#[cfg(not(feature = "tokio"))]
use async_std::{net::TcpStream, os::unix::net::UnixStream};
#[cfg(not(feature = "tokio"))]
use futures_util::{AsyncReadExt, AsyncWriteExt};
#[cfg(feature = "tokio")]
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    runtime::Runtime,
};
use tokio_socks::{tcp::socks5::Socks5Stream, Error};

const UNIX_PROXY_ADDR: &str = "/tmp/tor/socket.s";
const TCP_PROXY_ADDR: &str = "127.0.0.1:9050";
const ONION_ADDR: &str = "3g2upl4pq6kufc4m.onion:80"; // DuckDuckGo

async fn connect() -> Result<(), Error> {
    // This require Tor to listen on and Unix Domain Socket.
    // You have to create a directory /tmp/tor owned by tor, and for which only tor
    // has rights, and add the following line to your torrc :
    // SocksPort unix:/tmp/tor/socket.s
    let socket = UnixStream::connect(UNIX_PROXY_ADDR).await?;
    let target = Socks5Stream::tor_resolve_with_socket(socket, "duckduckgo.com:0").await?;
    eprintln!("duckduckgo.com = {:?}", target);
    let socket = UnixStream::connect(UNIX_PROXY_ADDR).await?;
    let target = Socks5Stream::tor_resolve_ptr_with_socket(socket, "176.34.155.23:0").await?;
    eprintln!("176.34.155.23 = {:?}", target);

    let socket = TcpStream::connect(TCP_PROXY_ADDR).await?;
    socket.set_nodelay(true)?;
    let mut conn = Socks5Stream::connect_with_socket(socket, ONION_ADDR).await?;
    conn.write_all(b"GET /\n\n").await?;

    let mut buf = Vec::new();
    let n = conn.read_to_end(&mut buf).await?;

    println!("{} bytes read\n\n{}", n, String::from_utf8_lossy(&buf));

    Ok(())
}

#[cfg(feature = "tokio")]
fn main() {
    let rt = Runtime::new().unwrap();
    rt.block_on(connect()).unwrap();
}

#[cfg(not(feature = "tokio"))]
fn main() {
    async_std::task::block_on(connect()).unwrap();
}
