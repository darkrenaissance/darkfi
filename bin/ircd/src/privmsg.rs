use rand::{rngs::OsRng, RngCore};

use darkfi::util::{
    serial::{SerialDecodable, SerialEncodable},
    Timestamp,
};

pub type PrivmsgId = u64;

#[derive(Debug, Clone, SerialEncodable, SerialDecodable, Eq, PartialEq)]
pub struct Privmsg {
    pub id: PrivmsgId,
    pub nickname: String,
    pub target: String,
    pub message: String,
    pub timestamp: Timestamp,
    pub term: u64,
}

impl Privmsg {
    pub fn new(nickname: String, target: String, message: String, term: u64) -> Self {
        let id = OsRng.next_u64();
        let timestamp = Timestamp::current_time();
        Self { id, nickname, target, message, timestamp, term }
    }
}

impl std::string::ToString for Privmsg {
    fn to_string(&self) -> String {
        format!(":{}!anon@dark.fi PRIVMSG {} :{}\r\n", self.nickname, self.target, self.message)
    }
}
