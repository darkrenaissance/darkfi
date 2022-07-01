use async_std::sync::Arc;
use std::{env, time::Duration};

use log::error;
use url::Url;

use crate::{Error, Result};

use super::{
    Channel, ChannelPtr, SessionWeakPtr, SettingsPtr, TcpTransport, TorTransport, Transport,
    TransportName,
};

/// Create outbound socket connections.
pub struct Connector {
    settings: SettingsPtr,
    //pub session: SessionWeakPtr,
}

impl Connector {
    /// Create a new connector with default network settings.
    pub fn new(settings: SettingsPtr, // session: SessionWeakPtr
    ) -> Self {
        Self { settings, // session
                        }
    }

    /// Establish an outbound connection.
    pub async fn connect(&self, connect_url: Url) -> Result<ChannelPtr> {
        let transport_name = TransportName::try_from(connect_url.clone())?;
        self.connect_channel(
            connect_url,
            transport_name,
            Duration::from_secs(self.settings.connect_timeout_seconds.into()),
        )
        .await
    }

    async fn connect_channel(
        &self,
        connect_url: Url,
        transport_name: TransportName,
        timeout: Duration,
    ) -> Result<Arc<Channel>> {
        macro_rules! connect {
            ($stream:expr, $transport:expr, $upgrade:expr) => {{
                if let Err(err) = $stream {
                    error!("Setup for {} failed: {}", connect_url, err);
                    return Err(Error::ConnectFailed)
                }

                let stream = $stream?.await;

                if let Err(err) = stream {
                    error!("Connection to {}  failed: {}", connect_url, err);
                    return Err(Error::ConnectFailed)
                }

                let channel = match $upgrade {
                    // session
                    None => Channel::new(Box::new(stream?), connect_url.clone()).await,
                    Some(u) if u == "tls" => {
                        let stream = $transport.upgrade_dialer(stream?)?.await;
                        Channel::new(Box::new(stream?), connect_url).await
                    }
                    Some(u) => return Err(Error::UnsupportedTransportUpgrade(u)),
                };

                Ok(channel)
            }};
        }

        match transport_name {
            TransportName::Tcp(upgrade) => {
                let transport = TcpTransport::new(None, 1024);
                let stream = transport.dial(connect_url.clone(), Some(timeout));
                connect!(stream, transport, upgrade)
            }
            TransportName::Tor(upgrade) => {
                let socks5_url = Url::parse(
                    &env::var("DARKFI_TOR_SOCKS5_URL")
                        .unwrap_or_else(|_| "socks5://127.0.0.1:9050".to_string()),
                )?;

                let transport = TorTransport::new(socks5_url, None)?;

                let stream = transport.clone().dial(connect_url.clone(), None);

                connect!(stream, transport, upgrade)
            }
            _ => unimplemented!(),
        }
    }
}
