use async_std::future::timeout;
use std::time::Duration;
use url::Url;

use crate::Result;

use super::{Channel, ChannelPtr, SettingsPtr, TcpTransport, TlsTransport, Transport};

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
    pub async fn connect(&self, connect_url: Url) -> Result<ChannelPtr> {
        let result =
            timeout(Duration::from_secs(self.settings.connect_timeout_seconds.into()), async {
                match connect_url.scheme() {
                    "tcp" => {
                        let transport = TcpTransport::new(None, 1024);
                        let stream = transport.dial(connect_url.clone())?.await?;
                        Ok(Channel::new(Box::new(stream), connect_url).await)
                    }
                    "tls" => {
                        let transport = TlsTransport::new(None, 1024);
                        let stream = transport.dial(connect_url.clone())?.await?;
                        Ok(Channel::new(Box::new(stream), connect_url).await)
                    }
                    "tor" => todo!(),
                    _ => unimplemented!(),
                }
            })
            .await?;
        result
    }
}
