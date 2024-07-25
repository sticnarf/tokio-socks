use super::AsyncSocket;
use futures_io::{AsyncRead, AsyncWrite};
use std::{
    io::Result as IoResult,
    ops::{Deref, DerefMut},
    pin::Pin,
    task::{Context, Poll},
};

pub struct Compat<S>(S);

impl<S> Compat<S> {
    pub fn new(inner: S) -> Self {
        Compat(inner)
    }

    pub fn into_inner(self) -> S {
        self.0
    }
}

impl<S> Deref for Compat<S> {
    type Target = S;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<S> DerefMut for Compat<S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub trait FuturesIoCompatExt {
    fn compat(self) -> Compat<Self>
    where Self: Sized;
}

impl<S> FuturesIoCompatExt for S
where S: AsyncRead + AsyncWrite + Unpin
{
    fn compat(self) -> Compat<Self> {
        Compat::new(self)
    }
}

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
