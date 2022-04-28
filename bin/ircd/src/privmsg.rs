use darkfi::util::serial::{SerialDecodable, SerialEncodable};

pub type PrivmsgId = u32;

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Privmsg {
    pub id: PrivmsgId,
    pub nickname: String,
    pub channel: String,
    pub message: String,
}
