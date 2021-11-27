use futures::FutureExt;
use smol::Async;
use std::net::{SocketAddr, TcpStream};

use crate::error::{Error, Result};
//use crate::net::error::{Error, Result};
use crate::net::{utility::sleep, Channel, ChannelPtr, SettingsPtr};

/// Create outbound socket connections.
pub struct Connector {
    settings: SettingsPtr,
}

impl Connector {
    /// Create a new connector with default network settings.
    pub fn new(settings: SettingsPtr) -> Self {
        Self { settings }
    }

    /// Establish an outbound connection.
    pub async fn connect(&self, hostaddr: SocketAddr) -> Result<ChannelPtr> {
        futures::select! {
            stream_result = Async::<TcpStream>::connect(hostaddr).fuse() => {
                match stream_result {
                    Ok(stream) => Ok(Channel::new(stream, hostaddr).await),
                    Err(_) => Err(Error::ConnectFailed)
                }
            }
            _ = sleep(self.settings.connect_timeout_seconds).fuse() => Err(Error::ConnectTimeout)
        }
    }
}
