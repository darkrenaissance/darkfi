use std::io;
use drk::{
    net,
    serial::{Decodable, Encodable}, Result,
};

pub type PrivMsgId = u32;

#[derive(Debug, Clone)]
pub struct PrivMsg {
    pub id: PrivMsgId,
    pub nickname: String,
    pub channel: String,
    pub message: String,
}

impl net::Message for PrivMsg {
    fn name() -> &'static str {
        "privmsg"
    }
}

impl Encodable for PrivMsg {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.id.encode(&mut s)?;
        len += self.nickname.encode(&mut s)?;
        len += self.channel.encode(&mut s)?;
        len += self.message.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for PrivMsg {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            id: Decodable::decode(&mut d)?,
            nickname: Decodable::decode(&mut d)?,
            channel: Decodable::decode(&mut d)?,
            message: Decodable::decode(&mut d)?,
        })
    }
}

