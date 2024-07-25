use super::{tokio_utils::runtime, *};
use futures_util::{io::copy, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream as StdTcpStream},
};
use tokio::net::{TcpListener, UnixStream};
use tokio_socks::{
    io::Compat,
    tcp::{socks4::Socks4Listener, socks5::Socks5Listener},
    Error,
    Result,
};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

pub async fn echo_server() -> Result<()> {
    let listener = TcpListener::bind(&SocketAddr::from(([0, 0, 0, 0], 10007))).await?;
    loop {
        let (mut stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            let (reader, writer) = stream.split();
            copy(&mut reader.compat(), &mut writer.compat_write()).await.unwrap();
        });
    }
}

pub async fn reply_response<S: AsyncRead + AsyncWrite + Unpin>(mut socket: S) -> Result<[u8; 5]> {
    socket.write_all(MSG).await?;
    let mut buf = [0; 5];
    socket.read_exact(&mut buf).await?;
    Ok(buf)
}

pub async fn test_connect<S: AsyncRead + AsyncWrite + Unpin>(socket: S) -> Result<()> {
    let res = reply_response(socket).await?;
    assert_eq!(&res[..], MSG);
    Ok(())
}

pub fn test_bind<S: 'static + AsyncRead + AsyncWrite + Unpin + Send>(
    listener: Socks5Listener<Compat<S>>,
) -> Result<()> {
    let bind_addr = listener.bind_addr().to_owned();
    runtime().lock().unwrap().spawn(async move {
        let stream = listener.accept().await.unwrap();
        let (mut reader, mut writer) = stream.split();
        copy(&mut reader, &mut writer).await.unwrap();
    });

    let mut tcp = StdTcpStream::connect(bind_addr)?;
    tcp.write_all(MSG)?;
    let mut buf = [0; 5];
    tcp.read_exact(&mut buf[..])?;
    assert_eq!(&buf[..], MSG);
    Ok(())
}

pub async fn connect_unix(proxy_addr: &str) -> Result<tokio_util::compat::Compat<UnixStream>> {
    UnixStream::connect(proxy_addr)
        .await
        .map_err(Error::Io)
        .map(|stream| stream.compat())
}

pub fn test_bind_socks4<S: 'static + AsyncRead + AsyncWrite + Unpin + Send>(
    listener: Socks4Listener<Compat<S>>,
) -> Result<()> {
    let bind_addr = listener.bind_addr().to_owned();
    runtime().lock().unwrap().spawn(async move {
        let stream = listener.accept().await.unwrap();
        let (mut reader, mut writer) = AsyncReadExt::split(stream);
        copy(&mut reader, &mut writer).await.unwrap();
    });

    let mut tcp = StdTcpStream::connect(bind_addr)?;
    tcp.write_all(MSG)?;
    let mut buf = [0; 5];
    tcp.read_exact(&mut buf[..])?;
    assert_eq!(&buf[..], MSG);
    Ok(())
}
