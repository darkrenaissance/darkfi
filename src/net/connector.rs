use async_std::future::timeout;
use smol::Async;
use std::{
    net::{SocketAddr, TcpStream},
    time::Duration,
};

use crate::{
    error::{Error, Result},
    net::{Channel, ChannelPtr, SettingsPtr},
};

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
        let stream_result =
            timeout(Duration::from_secs(self.settings.connect_timeout_seconds.into()), async {
                match Async::<TcpStream>::connect(hostaddr).await {
                    Ok(stream) => Ok(Channel::new(stream, hostaddr).await),
                    Err(_) => Err(Error::ConnectFailed),
                }
            })
            .await;
        match stream_result {
            Ok(t) => t,
            Err(_) => Err(Error::ConnectTimeout),
        }
    }
}
