//! Compat layer for `futures-io` types.
//!
//! This module provides a compatibility layer for using `futures-io` types with
//! `async-socks5`. AsyncSocket is implemented for Compat<S> where S is an
//! AsyncRead + AsyncWrite + Unpin type from `futures-io`.
use super::AsyncSocket;
use futures_io::{AsyncRead, AsyncWrite};
use std::{
    io::Result as IoResult,
    ops::{Deref, DerefMut},
    pin::Pin,
    task::{Context, Poll},
};

/// A compatibility layer for using `futures-io` types with `async-socks5`.
///
/// Use `FuturesIoCompatExt` to convert `futures-io` types to `Compat` types.
pub struct Compat<S>(S);

impl<S> Compat<S> {
    pub(crate) fn new(inner: S) -> Self {
        Compat(inner)
    }

    /// Unwraps this Compat, returning the inner value.
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

/// Import this trait to use socks with `futures-io` compatible runtime.
///
/// Example:
/// ```no_run
/// use async_std::os::unix::net::UnixStream;
/// use tokio_socks::{io::FuturesIoCompatExt as _, tcp::Socks5Stream};
///
/// let socket = UnixStream::connect(proxy_addr) // Compat<UnixStream>
///     .await
///     .map_err(Error::Io)?
///     .compat();
/// let conn =
///     Socks5Stream::connect_with_password_and_socket(socket, target, username, pswd).await?;
/// // Socks5Stream has implemented futures-io AsyncRead + AsyncWrite.
/// ```
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
