use super::channel::ChannelInfo;
use crate::util::time::NanoTimestamp;
use darkfi_serial::{SerialDecodable, SerialEncodable};

macro_rules! dnet {
    ($self:expr, $($code:tt)*) => {
        {
            if *$self.p2p().dnet_enabled.lock().await {
                $($code)*
            }
        }
    };
}
pub(crate) use dnet;

#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct MessageInfo {
    pub chan: ChannelInfo,
    pub cmd: String,
    pub time: NanoTimestamp,
}

#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub enum DnetEvent {
    SendMessage(MessageInfo),
}
