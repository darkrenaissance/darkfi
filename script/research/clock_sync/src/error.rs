#[derive(Debug, thiserror::Error)]
pub enum ClockError {
    #[error("AsyncNativeTls error: '{0}'")]
    AsyncNativeTlsError(String),
    #[error("FromUtf8 error: '{0}'")]
    FromUtf8Error(String),
    #[error("System clock is not correct!")]
    InvalidClock,
    #[error("Io error: '{0}'")]
    IoError(String),
    #[error("NTP error: '{0}'")]
    NtpError(String),
    #[error("SerdeJson error: '{0}'")]
    SerdeJsonError(String),
    #[error("SystemTime error: '{0}'")]
    SysTimeError(String),
}

pub type ClockResult<T> = std::result::Result<T, ClockError>;

impl From<async_native_tls::Error> for ClockError {
    fn from(err: async_native_tls::Error) -> ClockError {
        ClockError::AsyncNativeTlsError(err.to_string())
    }
}

impl From<std::string::FromUtf8Error> for ClockError {
    fn from(err: std::string::FromUtf8Error) -> ClockError {
        ClockError::FromUtf8Error(err.to_string())
    }
}

impl From<ntp::errors::Error> for ClockError {
    fn from(err: ntp::errors::Error) -> ClockError {
        ClockError::NtpError(err.to_string())
    }
}

impl From<serde_json::Error> for ClockError {
    fn from(err: serde_json::Error) -> ClockError {
        ClockError::SerdeJsonError(err.to_string())
    }
}

impl From<std::io::Error> for ClockError {
    fn from(err: std::io::Error) -> ClockError {
        ClockError::IoError(err.to_string())
    }
}

impl From<std::time::SystemTimeError> for ClockError {
    fn from(err: std::time::SystemTimeError) -> ClockError {
        ClockError::SysTimeError(err.to_string())
    }
}
