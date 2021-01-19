use smol::{Async, Executor, Timer};
use std::time::Duration;

use crate::error::Error;

pub async fn sleep(seconds: u32) {
    Timer::after(Duration::from_secs(seconds.into())).await;
}

pub fn clone_net_error(error: &Error) -> Error {
    match error {
        Error::ConnectFailed => Error::ConnectFailed,
        Error::ConnectTimeout => Error::ConnectTimeout,
        Error::ChannelStopped => Error::ChannelStopped,
        Error::ChannelTimeout => Error::ChannelTimeout,
        _ => Error::OperationFailed,
    }
}
