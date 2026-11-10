use std::fmt;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("no response from keyboard (feature index 0x{feature:02x}, function {function})")]
    Timeout { feature: u8, function: u8 },

    #[error("HID++ error {code} (feature index 0x{feature:02x}, function {function})")]
    Hidpp {
        feature: u8,
        function: u8,
        code: ErrorCode,
    },

    #[error("feature 0x{0:04x} is not supported by this keyboard")]
    Unsupported(u16),

    #[error("unexpected response: {0}")]
    Protocol(String),

    /// A safety check refused to write to the keyboard's flash.
    #[error("refusing to write flash: {0}")]
    Refused(String),
}

/// HID++ 2.0 error codes (byte 5 of an error report).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    NoError,
    Unknown,
    InvalidArgument,
    OutOfRange,
    HardwareError,
    LogitechInternal,
    InvalidFeatureIndex,
    InvalidFunctionId,
    Busy,
    Unsupported,
    Other(u8),
}

impl From<u8> for ErrorCode {
    fn from(code: u8) -> Self {
        match code {
            0 => Self::NoError,
            1 => Self::Unknown,
            2 => Self::InvalidArgument,
            3 => Self::OutOfRange,
            4 => Self::HardwareError,
            5 => Self::LogitechInternal,
            6 => Self::InvalidFeatureIndex,
            7 => Self::InvalidFunctionId,
            8 => Self::Busy,
            9 => Self::Unsupported,
            other => Self::Other(other),
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Other(code) => write!(f, "0x{code:02x}"),
            known => write!(f, "{known:?}"),
        }
    }
}
