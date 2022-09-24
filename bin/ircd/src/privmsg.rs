use chrono::Utc;
use rand::{rngs::OsRng, RngCore};

use darkfi::serial::{SerialDecodable, SerialEncodable};

pub type PrivmsgId = u64;

#[derive(Debug, Clone, SerialEncodable, SerialDecodable, Eq, PartialEq)]
pub struct Privmsg {
    pub id: PrivmsgId,
    pub nickname: String,
    pub target: String,
    pub message: String,
    pub timestamp: i64,
    pub term: u64,
    pub read_confirms: u8,
}

impl Privmsg {
    pub fn new(nickname: &str, target: &str, message: &str, term: u64) -> Self {
        let id = OsRng.next_u64();
        let timestamp = Utc::now().timestamp();
        let read_confirms = 0;
        Self {
            id,
            nickname: nickname.to_string(),
            target: target.to_string(),
            message: message.to_string(),
            timestamp,
            term,
            read_confirms,
        }
    }
}

impl std::string::ToString for Privmsg {
    fn to_string(&self) -> String {
        format!(":{}!anon@dark.fi PRIVMSG {} :{}\r\n", self.nickname, self.target, self.message)
    }
}
