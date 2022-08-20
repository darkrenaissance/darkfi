use async_std::sync::{Arc, Mutex};

use ringbuffer::{AllocRingBuffer, RingBufferExt, RingBufferWrite};

use darkfi::util::{
    serial::{SerialDecodable, SerialEncodable},
    Timestamp,
};

use crate::SIZE_OF_MSGS_BUFFER;

pub type PrivmsgId = u64;

pub type SeenMsgIds = Arc<Mutex<AllocRingBuffer<u64>>>;

pub type ArcPrivmsgsBuffer = Arc<Mutex<PrivmsgsBuffer>>;

pub struct PrivmsgsBuffer(AllocRingBuffer<Privmsg>);

impl PrivmsgsBuffer {
    pub fn new() -> ArcPrivmsgsBuffer {
        Arc::new(Mutex::new(Self(ringbuffer::AllocRingBuffer::with_capacity(SIZE_OF_MSGS_BUFFER))))
    }

    pub fn push(&mut self, privmsg: &Privmsg) {
        if privmsg.timestamp > Timestamp::current_time() {
            return
        }

        if let Some(last_msg) = self.0.get(-1) {
            if privmsg.timestamp > last_msg.timestamp {
                self.0.push(privmsg.clone());
            }
        } else {
            self.0.push(privmsg.clone());
        }
    }

    pub fn to_vec(&self) -> Vec<Privmsg> {
        self.0.to_vec()
    }
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Privmsg {
    pub id: PrivmsgId,
    pub nickname: String,
    pub target: String,
    pub message: String,
    pub timestamp: Timestamp,
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
