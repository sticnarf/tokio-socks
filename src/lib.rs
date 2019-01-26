use std::net::SocketAddr;

pub use error::Error;
use error::Result;

/// A trait for objects which can be converted or resolved to one or more `SocketAddr` values,
/// which are going to be connected as the the proxy server.
///
/// This trait is similar to `std::net::ToSocketAddrs` but allows asynchronous name resolution.
pub trait ToProxyAddrs {
    type Iter: Iterator<Item = SocketAddr>;

    fn to_addrs(&self) -> Result<Self::Iter>;
}

/// A SOCKS connection target.
#[derive(Debug)]
pub enum TargetAddr {
    /// Connect to an IP address.
    Ip(SocketAddr),

    /// Connect to a fully qualified domain name.
    ///
    /// The domain name will be passed along to the proxy server and DNS lookup will happen there.
    Domain(String, u16),
}

/// A trait for objects that can be converted to `TargetAddr`.
pub trait ToTargetAddr {
    /// Converts the value of self to a `TargetAddr`.
    fn to_target_addr(&self) -> Result<TargetAddr>;
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
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
