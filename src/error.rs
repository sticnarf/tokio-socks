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
}

/// Result type of `tokio-socks`
pub type Result<T> = std::result::Result<T, Error>;
