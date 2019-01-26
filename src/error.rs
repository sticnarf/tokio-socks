use failure::Fail;

/// Error type of `tokio-socks`
#[derive(Fail, Debug)]
pub enum Error {
    /// Failure caused by an IO error
    #[fail(display = "{}", _0)]
    Io(#[cause] std::io::Error),
}

/// Result type of `tokio-socks`
pub type Result<T> = std::result::Result<T, Error>;
