#[cfg(feature = "futures-io")]
mod futures;
#[cfg(feature = "tokio")]
mod tokio;

use futures_util::ready;
use std::{
    future::Future,
    io::{Error, ErrorKind},
    mem,
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

pub trait AsyncSocket {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<Result<usize, Error>>;

    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<Result<usize, Error>>;
}

pub(crate) trait AsyncSocketExt {
    fn read_exact<'a>(&'a mut self, buf: &'a mut [u8]) -> ReadExact<'a, Self>
    where Self: Sized;

    fn write_all<'a>(&'a mut self, buf: &'a [u8]) -> WriteAll<'a, Self>
    where Self: Sized;
}

impl<S: AsyncSocket> AsyncSocketExt for S {
    fn read_exact<'a>(&'a mut self, buf: &'a mut [u8]) -> ReadExact<'a, Self>
    where Self: Sized {
        let capacity = buf.len();
        ReadExact {
            reader: self,
            buf,
            capacity,
        }
    }

    fn write_all<'a>(&'a mut self, buf: &'a [u8]) -> WriteAll<'a, Self>
    where Self: Sized {
        WriteAll { writer: self, buf }
    }
}

pub(crate) struct ReadExact<'a, R> {
    reader: &'a mut R,
    buf: &'a mut [u8],
    capacity: usize,
}

impl<R: AsyncSocket + Unpin> Future for ReadExact<'_, R> {
    type Output = Result<usize, Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        while !this.buf.is_empty() {
            let n = ready!(Pin::new(&mut *this.reader).poll_read(cx, this.buf))?;
            {
                let (_, rest) = mem::take(&mut this.buf).split_at_mut(n);
                this.buf = rest;
            }
            if n == 0 {
                return Poll::Ready(Err(ErrorKind::UnexpectedEof.into()));
            }
        }
        Poll::Ready(Ok(this.capacity))
    }
}

pub(crate) struct WriteAll<'a, W> {
    writer: &'a mut W,
    buf: &'a [u8],
}

impl<W: AsyncSocket + Unpin> Future for WriteAll<'_, W> {
    type Output = Result<(), Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        while !this.buf.is_empty() {
            let n = ready!(Pin::new(&mut *this.writer).poll_write(cx, this.buf))?;
            {
                let (_, rest) = mem::take(&mut this.buf).split_at(n);
                this.buf = rest;
            }
            if n == 0 {
                return Poll::Ready(Err(ErrorKind::WriteZero.into()));
            }
        }

        Poll::Ready(Ok(()))
    }
}