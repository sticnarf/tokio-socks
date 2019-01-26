use futures::stream::{self, IterOk, Once, Stream};
use std::iter::Cloned;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::slice::Iter;

pub use error::Error;
use error::Result;

/// A trait for objects which can be converted or resolved to one or more `SocketAddr` values,
/// which are going to be connected as the the proxy server.
///
/// This trait is similar to `std::net::ToSocketAddrs` but allows asynchronous name resolution.
pub trait ToProxyAddrs {
    type Output: Stream<Item = SocketAddr, Error = Error>;

    fn to_proxy_addrs(&self) -> Self::Output;
}

macro_rules! trivial_impl_to_proxy_addrs {
    ($t: ty) => {
        impl ToProxyAddrs for $t {
            type Output = Once<SocketAddr, Error>;

            fn to_proxy_addrs(&self) -> Self::Output {
                stream::once(Ok(SocketAddr::from(*self)))
            }
        }
    };
}

trivial_impl_to_proxy_addrs!(SocketAddr);
trivial_impl_to_proxy_addrs!((IpAddr, u16));
trivial_impl_to_proxy_addrs!((Ipv4Addr, u16));
trivial_impl_to_proxy_addrs!((Ipv6Addr, u16));
trivial_impl_to_proxy_addrs!(SocketAddrV4);
trivial_impl_to_proxy_addrs!(SocketAddrV6);

impl<'a> ToProxyAddrs for &'a [SocketAddr] {
    type Output = IterOk<Cloned<Iter<'a, SocketAddr>>, Error>;

    fn to_proxy_addrs(&self) -> Self::Output {
        stream::iter_ok(self.iter().cloned())
    }
}

impl<'a, T: ToProxyAddrs + ?Sized> ToProxyAddrs for &'a T {
    type Output = T::Output;

    fn to_proxy_addrs(&self) -> Self::Output {
        (**self).to_proxy_addrs()
    }
}

/// A SOCKS connection target.
#[derive(Debug, PartialEq, Eq)]
pub enum TargetAddr<'a> {
    /// Connect to an IP address.
    Ip(SocketAddr),

    /// Connect to a fully qualified domain name.
    ///
    /// The domain name will be passed along to the proxy server and DNS lookup will happen there.
    Domain(&'a str, u16),
}

/// A trait for objects that can be converted to `TargetAddr`.
pub trait ToTargetAddr<'a> {
    /// Converts the value of self to a `TargetAddr`.
    fn to_target_addr<'b>(&'b self) -> Result<TargetAddr<'a>>;
}

macro_rules! trivial_impl_to_target_addr {
    ($t: ty) => {
        impl<'a> ToTargetAddr<'a> for $t {
            fn to_target_addr<'b>(&'b self) -> Result<TargetAddr<'a>> {
                Ok(TargetAddr::Ip(SocketAddr::from(*self)))
            }
        }
    };
}

trivial_impl_to_target_addr!(SocketAddr);
trivial_impl_to_target_addr!((IpAddr, u16));
trivial_impl_to_target_addr!((Ipv4Addr, u16));
trivial_impl_to_target_addr!((Ipv6Addr, u16));
trivial_impl_to_target_addr!(SocketAddrV4);
trivial_impl_to_target_addr!(SocketAddrV6);

impl<'a> ToTargetAddr<'a> for (&'a str, u16) {
    fn to_target_addr<'b>(&'b self) -> Result<TargetAddr<'a>> {
        // TODO: Validate domain
        Ok(TargetAddr::Domain(self.0, self.1))
    }
}

impl<'a> ToTargetAddr<'a> for &'a str {
    fn to_target_addr<'b>(&'b self) -> Result<TargetAddr<'a>> {
        unimplemented!()
    }
}

impl<'a, T> ToTargetAddr<'a> for &'a T
where
    T: ToTargetAddr<'a> + ?Sized,
{
    fn to_target_addr<'b>(&'b self) -> Result<TargetAddr<'a>> {
        (**self).to_target_addr()
    }
}

/// Authentication methods
#[derive(Debug)]
enum Authentication<'a> {
    Password {
        username: &'a str,
        password: &'a str,
    },
    None,
}

mod error;
pub mod tcp;

#[cfg(test)]
mod tests {
    use super::*;

    fn to_proxy_addrs<T: ToProxyAddrs>(t: T) -> Result<Vec<SocketAddr>> {
        t.to_proxy_addrs().wait().collect()
    }

    #[test]
    fn converts_socket_addr_to_proxy_addrs() -> Result<()> {
        let addr = SocketAddr::from(([1, 1, 1, 1], 443));
        let res = to_proxy_addrs(addr)?;
        assert_eq!(&res[..], &[addr]);
        Ok(())
    }

    #[test]
    fn converts_socket_addr_ref_to_proxy_addrs() -> Result<()> {
        let addr = SocketAddr::from(([1, 1, 1, 1], 443));
        let res = to_proxy_addrs(&addr)?;
        assert_eq!(&res[..], &[addr]);
        Ok(())
    }

    #[test]
    fn converts_socket_addrs_to_proxy_addrs() -> Result<()> {
        let addrs = [
            SocketAddr::from(([1, 1, 1, 1], 443)),
            SocketAddr::from(([8, 8, 8, 8], 53)),
        ];
        let res = to_proxy_addrs(&addrs[..])?;
        assert_eq!(&res[..], &addrs);
        Ok(())
    }

    fn to_target_addr<'a, T>(t: T) -> Result<TargetAddr<'a>>
    where
        T: ToTargetAddr<'a> + 'a,
    {
        t.to_target_addr()
    }

    #[test]
    fn converts_socket_addr_to_target_addr() -> Result<()> {
        let addr = SocketAddr::from(([1, 1, 1, 1], 443));
        let res = to_target_addr(addr)?;
        assert_eq!(TargetAddr::Ip(addr), res);
        Ok(())
    }

    #[test]
    fn converts_socket_addr_ref_to_target_addr() -> Result<()> {
        let addr = SocketAddr::from(([1, 1, 1, 1], 443));
        let res = to_target_addr(&addr)?;
        assert_eq!(TargetAddr::Ip(addr), res);
        Ok(())
    }

    #[test]
    fn converts_domain_to_target_addr() -> Result<()> {
        unimplemented!()
    }
}
