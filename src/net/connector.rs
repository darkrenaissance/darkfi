use futures::FutureExt;
use smol::Async;
use std::net::{SocketAddr, TcpStream};

use crate::net::error::{NetError, NetResult};
use crate::net::utility::sleep;
use crate::net::{Channel, ChannelPtr, SettingsPtr};

pub struct Connector {
    settings: SettingsPtr,
}

impl Connector {
    pub fn new(settings: SettingsPtr) -> Self {
        Self { settings }
    }

    pub async fn connect(&self, hostaddr: SocketAddr) -> NetResult<ChannelPtr> {
        futures::select! {
            stream_result = Async::<TcpStream>::connect(hostaddr).fuse() => {
                match stream_result {
                    Ok(stream) => Ok(Channel::new(stream, hostaddr, self.settings.clone()).await),
                    Err(_) => Err(NetError::ConnectFailed)
                }
            }
            _ = sleep(self.settings.connect_timeout_seconds).fuse() => Err(NetError::ConnectTimeout)
        }
    }
}
