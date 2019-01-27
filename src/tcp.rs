use crate::{Authentication, Error, Result, TargetAddr, ToProxyAddrs, ToTargetAddr};
use futures::{try_ready, Async, Future, Poll, Stream};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::borrow::Cow;
use tokio_io::{AsyncRead, AsyncWrite};
use tokio_tcp::{ConnectFuture as TokioConnect, TcpStream};

#[derive(Debug)]
pub struct Socks5Stream {
    tcp: TcpStream,
    target: TargetAddr<'static>,
}

impl Socks5Stream {
    /// Connects to a target server through a SOCKS5 proxy.
    ///
    /// # Error
    ///
    /// It propagates the error that occurs in the conversion from `T` to `TargetAddr`.
    pub fn connect<'t, P, T>(proxy: P, target: T) -> Result<ConnectFuture<'static, 't, P::Output>>
    where
        P: ToProxyAddrs,
        T: ToTargetAddr<'t>,
    {
        Ok(ConnectFuture::new(
            Authentication::None,
            proxy.to_proxy_addrs(),
            target.to_target_addr()?,
        ))
    }

    /// Connects to a target server through a SOCKS5 proxy using given username and password.
    ///
    /// # Error
    ///
    /// It propagates the error that occurs in the conversion from `T` to `TargetAddr`.
    pub fn connect_with_password<'a, 't, P, T>(
        proxy: P,
        target: T,
        username: &'a str,
        password: &'a str,
    ) -> Result<ConnectFuture<'a, 't, P::Output>>
    where
        P: ToProxyAddrs,
        T: ToTargetAddr<'t>,
    {
        Ok(ConnectFuture::new(
            Authentication::Password { username, password },
            proxy.to_proxy_addrs(),
            target.to_target_addr()?,
        ))
    }

    /// Consumes the `Socks5Stream`, returning the inner `tokio_tcp::TcpStream`.
    pub fn into_inner(self) -> TcpStream {
        self.tcp
    }

    /// Returns the target address that the proxy server connects to.
    pub fn target_addr(&self) -> TargetAddr<'_> {
        match &self.target {
            TargetAddr::Ip(addr) => TargetAddr::Ip(*addr),
            TargetAddr::Domain(domain, port) => {
                TargetAddr::Domain(Cow::clone(domain), *port)
            }
        }
    }
}

pub struct ConnectFuture<'a, 't, S>
where
    S: Stream<Item = SocketAddr, Error = Error>,
{
    auth: Authentication<'a>,
    proxy: S,
    target: TargetAddr<'t>,
    state: ConnectState,
    buf: [u8; 262],
    ptr: usize,
    len: usize,
}

impl<'a, 't, S> ConnectFuture<'a, 't, S>
where
    S: Stream<Item = SocketAddr, Error = Error>,
{
    fn new(auth: Authentication<'a>, proxy: S, target: TargetAddr<'t>) -> Self {
        ConnectFuture {
            auth,
            proxy,
            target,
            state: ConnectState::Uninitialized,
            buf: [0; 262],
            ptr: 0,
            len: 0,
        }
    }

    fn prepare_send_method_selection(&mut self) {
        self.ptr = 0;
        self.buf[0] = 0x05;
        match self.auth {
            Authentication::None => {
                self.buf[1..3].copy_from_slice(&[1, 0x00]);
                self.len = 3;
            }
            Authentication::Password { .. } => {
                self.buf[1..4].copy_from_slice(&[1, 0x00, 0x02]);
                self.len = 4;
            }
        }
    }

    fn prepare_recv_method_selection(&mut self) {
        self.ptr = 0;
        self.len = 2;
    }

    fn prepare_send_request(&mut self) {
        self.ptr = 0;
        self.buf[..3].copy_from_slice(&[0x05, 0x01, 0x00]);
        match &self.target {
            TargetAddr::Ip(SocketAddr::V4(addr)) => {
                self.buf[3] = 0x01;
                self.buf[4..8].copy_from_slice(&addr.ip().octets());
                self.buf[8..10].copy_from_slice(&addr.port().to_be_bytes());
                self.len = 10;
            }
            TargetAddr::Ip(SocketAddr::V6(addr)) => {
                self.buf[3] = 0x04;
                self.buf[4..20].copy_from_slice(&addr.ip().octets());
                self.buf[20..22].copy_from_slice(&addr.port().to_be_bytes());
                self.len = 22;
            }
            TargetAddr::Domain(domain, port) => {
                self.buf[3] = 0x03;
                let domain = domain.as_bytes();
                let len = domain.len();
                self.buf[4] = len as u8;
                self.buf[5..5 + len].copy_from_slice(domain);
                self.buf[(5 + len)..(7 + len)].copy_from_slice(&port.to_be_bytes());
                self.len = 7 + len;
            }
        }
    }

    fn prepare_recv_reply(&mut self) {
        self.ptr = 0;
        self.len = 4;
    }
}

impl<'a, 't, S> Future for ConnectFuture<'a, 't, S>
where
    S: Stream<Item = SocketAddr, Error = Error>,
{
    type Item = Socks5Stream;
    type Error = Error;

    fn poll(&mut self) -> Poll<Socks5Stream, Error> {
        loop {
            match self.state {
                ConnectState::Uninitialized => match try_ready!(self.proxy.poll()) {
                    Some(addr) => self.state = ConnectState::Created(TcpStream::connect(&addr)),
                    None => Err(Error::ProxyServerUnreachable)?,
                },
                ConnectState::Created(ref mut conn_fut) => match conn_fut.poll() {
                    Ok(Async::Ready(tcp)) => {
                        self.state = ConnectState::Connected(Some(tcp));
                        self.prepare_send_method_selection()
                    }
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Err(e) => self.state = ConnectState::Uninitialized,
                },
                ConnectState::Connected(ref mut opt) => {
                    let tcp = opt.as_mut().unwrap();
                    let written = try_ready!(tcp.poll_write(&self.buf[self.ptr..self.len]));
                    self.ptr += written;
                    if self.ptr == self.len {
                        self.state = ConnectState::MethodSent(opt.take());
                        self.prepare_recv_method_selection();
                    }
                }
                ConnectState::MethodSent(ref mut opt) => {
                    let tcp = opt.as_mut().unwrap();
                    let read = try_ready!(tcp.poll_read(&mut self.buf[self.ptr..self.len]));
                    self.ptr += read;
                    if self.ptr == self.len {
                        if self.buf[0] != 0x05 {
                            Err(Error::InvalidResponseVersion)?
                        }
                        match self.buf[1] {
                            0x00 => self.state = ConnectState::PrepareRequest(opt.take()),
                            0xff => Err(Error::NoAcceptableAuthMethods)?,
                            m if m != self.auth.id() => Err(Error::UnknownAuthMethod)?,
                            _ => self.state = ConnectState::SubNegotiation(opt.take()),
                        }
                    }
                }
                ConnectState::SubNegotiation(ref mut opt) => {
                    match self.auth {
                        Authentication::Password { username, password } => unimplemented!(),
                        Authentication::None => unreachable!(),
                    }
                    self.state = ConnectState::PrepareRequest(opt.take());
                }
                ConnectState::PrepareRequest(ref mut opt) => {
                    self.state = ConnectState::SendRequest(opt.take());
                    self.prepare_send_request();
                }
                ConnectState::SendRequest(ref mut opt) => {
                    let tcp = opt.as_mut().unwrap();
                    let written = try_ready!(tcp.poll_write(&self.buf[self.ptr..self.len]));
                    self.ptr += written;
                    if self.ptr == self.len {
                        self.state = ConnectState::RequestSent(opt.take());
                        self.prepare_recv_reply();
                    }
                }
                ConnectState::RequestSent(ref mut opt) => {
                    let tcp = opt.as_mut().unwrap();
                    let read = try_ready!(tcp.poll_read(&mut self.buf[self.ptr..self.len]));
                    self.ptr += read;
                    if self.ptr == self.len {
                        if self.buf[0] != 0x05 {
                            Err(Error::InvalidResponseVersion)?
                        }
                        if self.buf[2] != 0x00 {
                            Err(Error::InvalidReservedByte)?
                        }
                        match self.buf[1] {
                            0x00 => {} // succeeded
                            0x01 => Err(Error::GeneralSocksServerFailure)?,
                            0x02 => Err(Error::ConnectionNotAllowedByRuleset)?,
                            0x03 => Err(Error::NetworkUnreachable)?,
                            0x04 => Err(Error::HostUnreachable)?,
                            0x05 => Err(Error::ConnectionRefused)?,
                            0x06 => Err(Error::TtlExpired)?,
                            0x07 => Err(Error::CommandNotSupported)?,
                            0x08 => Err(Error::AddressTypeNotSupported)?,
                            _ => Err(Error::UnknownAuthMethod)?,
                        }
                        match self.buf[3] {
                            // IPv4
                            0x01 => {
                                self.len = 10;
                                self.state = ConnectState::ReadAddress(opt.take())
                            }
                            // IPv6
                            0x04 => {
                                self.len = 22;
                                self.state = ConnectState::ReadAddress(opt.take())
                            }
                            // Domain
                            0x03 => {
                                self.len = 5;
                                self.state = ConnectState::PrepareReadAddress(opt.take())
                            }
                            _ => Err(Error::UnknownAddressType)?,
                        }
                    }
                }
                ConnectState::PrepareReadAddress(ref mut opt) => {
                    let tcp = opt.as_mut().unwrap();
                    let read = try_ready!(tcp.poll_read(&mut self.buf[self.ptr..self.len]));
                    self.ptr += read;
                    if self.ptr == self.len {
                        self.len += self.buf[4] as usize + 2;
                        self.state = ConnectState::ReadAddress(opt.take());
                    }
                }
                ConnectState::ReadAddress(ref mut opt) => {
                    let tcp = opt.as_mut().unwrap();
                    let read = try_ready!(tcp.poll_read(&mut self.buf[self.ptr..self.len]));
                    self.ptr += read;
                    if self.ptr == self.len {
                        let target: TargetAddr<'static> = match self.buf[3] {
                            // IPv4
                            0x01 => {
                                let mut ip = [0; 4];
                                ip[..].copy_from_slice(&self.buf[4..8]);
                                let ip = Ipv4Addr::from(ip);
                                let port = read_port(&self.buf[8..10]);
                                (ip, port).to_target_addr()?
                            }
                            // IPv6
                            0x04 => {
                                let mut ip = [0; 16];
                                ip[..].copy_from_slice(&self.buf[4..20]);
                                let ip = Ipv6Addr::from(ip);
                                let port = read_port(&self.buf[20..22]);
                                (ip, port).to_target_addr()?
                            }
                            // Domain
                            0x03 => {
                                let domain_bytes = (&self.buf[5..(self.len - 2)]).to_vec();
                                let domain = String::from_utf8(domain_bytes).map_err(|_| {
                                    Error::InvalidTargetAddress("not a valid UTF-8 string")
                                })?;
                                let port = read_port(&self.buf[(self.len - 2)..self.len]);
                                TargetAddr::Domain(domain.into(), port)
                            }
                            _ => unreachable!(),
                        };
                        return Ok(Async::Ready(Socks5Stream {
                            tcp: opt.take().unwrap(),
                            target,
                        }));
                    }
                }
            }
        }
    }
}

#[derive(Debug)]
enum ConnectState {
    Uninitialized,
    Created(TokioConnect),
    Connected(Option<TcpStream>),
    MethodSent(Option<TcpStream>),
    SubNegotiation(Option<TcpStream>),
    PrepareRequest(Option<TcpStream>),
    SendRequest(Option<TcpStream>),
    RequestSent(Option<TcpStream>),
    PrepareReadAddress(Option<TcpStream>),
    ReadAddress(Option<TcpStream>),
}

fn read_port(buf: &[u8]) -> u16 {
    assert_eq!(buf.len(), 2);
    u16::from_be_bytes([buf[0], buf[1]])
}