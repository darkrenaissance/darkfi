use async_std::future::timeout;
use std::time::Duration;

use url::Url;

use crate::{Error, Result};

use super::{Channel, ChannelPtr, SettingsPtr, Transport};

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
    pub async fn connect<T: Transport>(&self, hostaddr: Url) -> Result<ChannelPtr<T>> {
        let stream_result =
            timeout(Duration::from_secs(self.settings.connect_timeout_seconds.into()), async {
                let transport = T::new(None, 1024);
                let connect_stream = transport.dial(hostaddr.clone()).unwrap().await.unwrap();
                let channel = Channel::<T>::new(connect_stream, hostaddr).await;
                Ok(channel)
            })
            .await;

        match stream_result {
            Ok(t) => t,
            Err(_) => Err(Error::ConnectTimeout),
        }
    }
}
