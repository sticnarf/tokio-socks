#[cfg(not(feature = "tokio"))]
pub(crate) use futures_util::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
#[cfg(feature = "tokio")]
pub(crate) use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
