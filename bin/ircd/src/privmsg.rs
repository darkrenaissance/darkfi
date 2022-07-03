use async_std::sync::{Arc, Mutex};

use ringbuffer::AllocRingBuffer;

use darkfi::util::serial::{SerialDecodable, SerialEncodable};

pub type PrivmsgId = u64;

pub type SeenMsgIds = Arc<Mutex<AllocRingBuffer<u64>>>;

pub type PrivmsgsBuffer = Arc<Mutex<AllocRingBuffer<Privmsg>>>;

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Privmsg {
    pub id: PrivmsgId,
    pub nickname: String,
    pub target: String,
    pub message: String,
}

impl Privmsg {
    pub fn to_irc_msg(&self) -> String {
        let irc_msg = format!(
            ":{}!anon@dark.fi PRIVMSG {} :{}\r\n",
            self.nickname, self.target, self.message
        );
        irc_msg
    }
}
