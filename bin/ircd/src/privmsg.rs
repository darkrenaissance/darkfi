use async_std::sync::{Arc, Mutex};

use darkfi::util::serial::{SerialDecodable, SerialEncodable};

pub type PrivmsgId = u64;

pub type SeenMsgIds = Arc<Mutex<Vec<u64>>>;

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Privmsg {
    pub id: PrivmsgId,
    pub nickname: String,
    pub channel: String,
    pub message: String,
}
