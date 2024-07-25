use super::{AsyncSocket, Compat};
use futures_io::{AsyncRead, AsyncWrite};
use std::{
    io::Result as IoResult,
    ops::DerefMut,
    pin::Pin,
    task::{Context, Poll},
};

impl<S> AsyncSocket for Compat<S>
where S: AsyncRead + AsyncWrite + Unpin
{
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<IoResult<usize>> {
        AsyncRead::poll_read(Pin::new(self.get_mut().deref_mut()), cx, buf)
    }

    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<IoResult<usize>> {
        AsyncWrite::poll_write(Pin::new(self.get_mut().deref_mut()), cx, buf)
    }
}

impl<S> AsyncRead for Compat<S>
where S: AsyncRead + Unpin
{
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<IoResult<usize>> {
        AsyncRead::poll_read(Pin::new(self.get_mut().deref_mut()), cx, buf)
    }
}

impl<S> AsyncWrite for Compat<S>
where S: AsyncWrite + Unpin
{
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<IoResult<usize>> {
        AsyncWrite::poll_write(Pin::new(self.get_mut().deref_mut()), cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        AsyncWrite::poll_flush(Pin::new(self.get_mut().deref_mut()), cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        AsyncWrite::poll_close(Pin::new(self.get_mut().deref_mut()), cx)
    }
}
