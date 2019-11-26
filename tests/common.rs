use futures::{future, StreamExt};
use once_cell::sync::OnceCell;
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream as StdTcpStream},
    sync::Mutex,
};
use tokio::{io::AsyncReadExt, net::TcpListener, runtime::Runtime};
use tokio_io::AsyncWriteExt;
use tokio_socks::{
    tcp::{Socks5Listener, Socks5Stream},
    Result,
};

pub const PROXY_ADDR: &'static str = "127.0.0.1:41080";
pub const ECHO_SERVER_ADDR: &'static str = "localhost:10007";
pub const MSG: &[u8] = b"hello";

pub async fn echo_server() -> Result<()> {
    let listener = TcpListener::bind(&SocketAddr::from(([0, 0, 0, 0], 10007))).await?;
    listener
        .incoming()
        .for_each(|tcp_stream| {
            if let Ok(mut stream) = tcp_stream {
                tokio::spawn(async move {
                    let (mut reader, mut writer) = stream.split();
                    reader.copy(&mut writer).await.unwrap();
                });
            }

            future::ready(())
        })
        .await;
    Ok(())
}

pub async fn reply_response(mut socket: Socks5Stream) -> Result<[u8; 5]> {
    socket.write_all(MSG).await?;
    let mut buf = [0; 5];
    socket.read_exact(&mut buf).await?;
    Ok(buf)
}

pub async fn test_connect(socket: Socks5Stream) -> Result<()> {
    let res = reply_response(socket).await?;
    assert_eq!(&res[..], MSG);
    Ok(())
}

pub fn test_bind(listener: Socks5Listener) -> Result<()> {
    let bind_addr = listener.bind_addr().to_owned();
    runtime().lock().unwrap().spawn(async move {
        let mut stream = listener.accept().await.unwrap();
        let (mut reader, mut writer) = stream.split();
        reader.copy(&mut writer).await.unwrap();
    });

    let mut tcp = StdTcpStream::connect(bind_addr)?;
    tcp.write_all(MSG)?;
    let mut buf = [0; 5];
    tcp.read_exact(&mut buf[..])?;
    assert_eq!(&buf[..], MSG);
    Ok(())
}

pub fn runtime() -> &'static Mutex<Runtime> {
    static RUNTIME: OnceCell<Mutex<Runtime>> = OnceCell::new();
    RUNTIME.get_or_init(|| {
        let runtime = Runtime::new().expect("Unable to create runtime");
        runtime.spawn(async { echo_server().await.expect("Unable to bind") });
        Mutex::new(runtime)
    })
}
