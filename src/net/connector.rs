use futures::FutureExt;
use log::*;
use smol::{Async, Executor};
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;

use crate::error::{Error, Result};
use crate::net::utility::sleep;
use crate::net::{Channel, ChannelPtr, SettingsPtr};

pub struct Connector {
    settings: SettingsPtr,
}

impl Connector {
    pub fn new(settings: SettingsPtr) -> Self {
        Self { settings }
    }

    pub async fn connect(&self, hostaddr: SocketAddr) -> Result<ChannelPtr> {
        futures::select! {
            stream_result = Async::<TcpStream>::connect(hostaddr).fuse() => {
                match stream_result {
                    Ok(stream) => Ok(Channel::new(stream, hostaddr, self.settings.clone())),
                    Err(_) => Err(Error::ConnectFailed)
                }
            }
            _ = sleep(self.settings.connect_timeout_seconds).fuse() => Err(Error::ConnectTimeout)
        }
    }
}
