use futures::stream::{self, IterOk, Once, Stream};
use std::borrow::Cow;
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

    /// Connect to a fully-qualified domain name.
    ///
    /// The domain name will be passed along to the proxy server and DNS lookup will happen there.
    Domain(Cow<'a, str>, u16),
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
        // Try IP address first
        if let Ok(addr) = self.0.parse::<IpAddr>() {
            return (addr, self.1).to_target_addr();
        }

        // Treat as domain name
        let len = self.0.as_bytes().len();
        if len > 255 {
            return Err(Error::InvalidTargetAddress("overlong domain"));
        }
        // TODO: Should we validate the domain format here?

        Ok(TargetAddr::Domain(self.0.into(), self.1))
    }
}

impl<'a> ToTargetAddr<'a> for &'a str {
    fn to_target_addr<'b>(&'b self) -> Result<TargetAddr<'a>> {
        // Try IP address first
        if let Ok(addr) = self.parse::<SocketAddr>() {
            return addr.to_target_addr();
        }

        let mut parts_iter = self.rsplitn(2, ':');
        let port: u16 = parts_iter
            .next()
            .and_then(|port_str| port_str.parse().ok())
            .ok_or(Error::InvalidTargetAddress("invalid address format"))?;
        let domain = parts_iter
            .next()
            .ok_or(Error::InvalidTargetAddress("invalid address format"))?;
        (domain, port).to_target_addr()
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

impl<'a> Authentication<'a> {
    fn id(&self) -> u8 {
        match self {
            Authentication::Password { .. } => 0x02,
            Authentication::None => 0x00,
        }
    }
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
        T: ToTargetAddr<'a>,
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
    fn converts_socket_addr_str_to_target_addr() -> Result<()> {
        let addr = SocketAddr::from(([1, 1, 1, 1], 443));
        let ip_str = format!("{}", addr);
        let res = to_target_addr(ip_str.as_str())?;
        assert_eq!(TargetAddr::Ip(addr), res);
        Ok(())
    }

    #[test]
    fn converts_ip_str_and_port_target_addr() -> Result<()> {
        let addr = SocketAddr::from(([1, 1, 1, 1], 443));
        let ip_str = format!("{}", addr.ip());
        let res = to_target_addr((ip_str.as_str(), addr.port()))?;
        assert_eq!(TargetAddr::Ip(addr), res);
        Ok(())
    }

    #[test]
    fn converts_domain_to_target_addr() -> Result<()> {
        let domain = "www.example.com:80";
        let res = to_target_addr(domain)?;
        assert_eq!(
            TargetAddr::Domain(Cow::Borrowed("www.example.com"), 80),
            res
        );
        Ok(())
    }

    #[test]
    fn converts_domain_and_port_to_target_addr() -> Result<()> {
        let domain = "www.example.com";
        let res = to_target_addr((domain, 80))?;
        assert_eq!(
            TargetAddr::Domain(Cow::Borrowed("www.example.com"), 80),
            res
        );
        Ok(())
    }

    #[test]
    fn overlong_domain_to_target_addr_should_fail() {
        let domain = format!("www.{:a<1$}.com:80", 'a', 300);
        assert!(to_target_addr(domain.as_str()).is_err());
        let domain = format!("www.{:a<1$}.com", 'a', 300);
        assert!(to_target_addr((domain.as_str(), 80)).is_err());
    }

    #[test]
    fn addr_with_invalid_port_to_target_addr_should_fail() {
        let addr = "[ffff::1]:65536";
        assert!(to_target_addr(addr).is_err());
        let addr = "www.example.com:65536";
        assert!(to_target_addr(addr).is_err());
    }
}
