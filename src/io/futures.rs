use std::{
    io::Result as IoResult,
    pin::Pin,
    task::{Context, Poll},
};

use futures_io::{AsyncRead, AsyncWrite};

use super::{AsyncSocket, Compat};

impl<S> AsyncSocket for Compat<S>
where
    S: AsyncRead + AsyncWrite,
{
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<IoResult<usize>> {
        unsafe { AsyncRead::poll_read(self.map_unchecked_mut(|s| &mut s.0), cx, buf) }
    }

    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<IoResult<usize>> {
        unsafe { AsyncWrite::poll_write(self.map_unchecked_mut(|s| &mut s.0), cx, buf) }
    }
}
