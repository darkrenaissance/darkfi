use std::fmt;

pub type NetResult<T> = std::result::Result<T, NetError>;

#[derive(Debug, Copy, Clone)]
pub enum NetError {
    OperationFailed,
    ConnectFailed,
    ConnectTimeout,
    ChannelStopped,
    ChannelTimeout,
    ServiceStopped,
}

impl std::error::Error for NetError {}

impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        match *self {
            NetError::OperationFailed => f.write_str("Operation failed"),
            NetError::ConnectFailed => f.write_str("Connection failed"),
            NetError::ConnectTimeout => f.write_str("Connection timed out"),
            NetError::ChannelStopped => f.write_str("Channel stopped"),
            NetError::ChannelTimeout => f.write_str("Channel timed out"),
            NetError::ServiceStopped => f.write_str("Service stopped"),
        }
    }
}

