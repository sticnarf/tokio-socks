use failure::Fail;

/// Error type of `tokio-socks`
#[derive(Fail, Debug)]
pub enum Error {
    /// Failure caused by an IO error.
    #[fail(display = "{}", _0)]
    Io(#[cause] std::io::Error),
    /// Failure when parsing a `String`.
    #[fail(display = "{}", _0)]
    ParseError(#[cause] std::string::ParseError),
    /// Failure due to invalid target address.
    #[fail(display = "Target address is invalid: {}", _0)]
    InvalidTargetAddress(&'static str),
    /// Proxy server unreachable.
    #[fail(display = "Proxy server unreachable")]
    ProxyServerUnreachable,
    /// Proxy server returns an invalid version number.
    #[fail(display = "Invalid response version")]
    InvalidResponseVersion,
    /// No acceptable auth methods
    #[fail(display = "No acceptable auth methods")]
    NoAcceptableAuthMethods,
    /// Unknown auth method
    #[fail(display = "Unknown auth method")]
    UnknownAuthMethod,
    /// General SOCKS server failure
    #[fail(display = "General SOCKS server failure")]
    GeneralSocksServerFailure,
    /// Connection not allowed by ruleset
    #[fail(display = "Connection not allowed by ruleset")]
    ConnectionNotAllowedByRuleset,
    /// Network unreachable
    #[fail(display = "Network unreachable")]
    NetworkUnreachable,
    /// Host unreachable
    #[fail(display = "Host unreachable")]
    HostUnreachable,
    /// Connection refused
    #[fail(display = "Connection refused")]
    ConnectionRefused,
    /// TTL expired
    #[fail(display = "TTL expired")]
    TtlExpired,
    /// Command not supported
    #[fail(display = "Command not supported")]
    CommandNotSupported,
    /// Address type not supported
    #[fail(display = "Address type not supported")]
    AddressTypeNotSupported,
    /// Unknown error
    #[fail(display = "Unknown error")]
    UnknownError,
    /// Invalid reserved byte
    #[fail(display = "Invalid reserved byte")]
    InvalidReservedByte,
    /// Unknown address type
    #[fail(display = "Unknown address type")]
    UnknownAddressType,
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Error {
        Error::Io(err)
    }
}

/// Result type of `tokio-socks`
pub type Result<T> = std::result::Result<T, Error>;
