use std::fmt;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    /// The sampler answered with an error confirmation.
    Sampler {
        section: u8,
        item: u8,
        code: u16,
    },
    /// A reply could not be parsed or did not match the request.
    Protocol(String),
    Transfer(s5k_protocol::TransferError),
    Format(s5k_formats::Error),
    /// The request was well formed but cannot be carried out.
    Invalid(String),
}

impl Error {
    pub fn is_timeout(&self) -> bool {
        matches!(self, Error::Io(e) if e.kind() == std::io::ErrorKind::TimedOut)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                write!(f, "the sampler did not answer in time")
            }
            Error::Io(e) => write!(f, "USB: {e}"),
            Error::Sampler { section, item, code } => write!(
                f,
                "the sampler refused {section:02X} {item:02X}: {} (error {code:#x})",
                s5k_protocol::sysex::error_text(*code)
            ),
            Error::Protocol(m) => write!(f, "protocol: {m}"),
            Error::Transfer(e) => write!(f, "transfer: {e}"),
            Error::Format(e) => write!(f, "file: {e}"),
            Error::Invalid(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<s5k_protocol::TransferError> for Error {
    fn from(e: s5k_protocol::TransferError) -> Self {
        Error::Transfer(e)
    }
}

impl From<s5k_formats::Error> for Error {
    fn from(e: s5k_formats::Error) -> Self {
        Error::Format(e)
    }
}

impl From<s5k_protocol::sysex::ParseError> for Error {
    fn from(e: s5k_protocol::sysex::ParseError) -> Self {
        Error::Protocol(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
