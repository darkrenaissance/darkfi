use std::io;

use crate::{serial::{Decodable, Encodable}, Result};

#[derive(Copy, Clone)]
pub enum ControlCommand {
    Join = 0,
    Leave = 1,
    Message = 2,
}

pub struct MessagePayload {
    pub nickname: String,
    pub text: String,
    pub timestamp: i64,
}

pub struct ControlMessage {
    pub control: ControlCommand,
    pub payload: MessagePayload,
}

impl Encodable for MessagePayload {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.nickname.encode(&mut s)?;
        len += self.text.encode(&mut s)?;
        len += self.timestamp.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for MessagePayload {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            nickname: Decodable::decode(&mut d)?,
            text: Decodable::decode(&mut d)?,
            timestamp: Decodable::decode(&mut d)?,
        })
    }
}
impl Encodable for ControlMessage {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += (self.control as u8).encode(&mut s)?;
        len += self.payload.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for ControlMessage {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        let control_code: u8 = Decodable::decode(&mut d)?;
        let control = match control_code {
            0 => ControlCommand::Join,
            1 => ControlCommand::Leave,
            _ => ControlCommand::Message,
        };
        Ok(Self {
            control,
            payload: Decodable::decode(&mut d)?,
        })
    }
}
