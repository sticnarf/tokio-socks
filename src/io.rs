#[cfg(not(feature = "tokio"))]
pub(crate) use futures_util::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
#[cfg(feature = "tokio")]
pub(crate) use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

#[cfg(not(feature = "tokio"))]
pub async fn read_exact<S: AsyncRead + AsyncWrite + Unpin>(stream: &mut S, buf: &mut [u8]) -> std::io::Result<usize> {
    stream.read_exact(buf).await?;
    Ok(buf.len())
}

#[cfg(feature = "tokio")]
pub async fn read_exact<S: AsyncRead + AsyncWrite + Unpin>(stream: &mut S, buf: &mut [u8]) -> std::io::Result<usize> {
    stream.read_exact(buf).await
}
