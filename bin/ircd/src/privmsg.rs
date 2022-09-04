use chrono::Utc;
use rand::{rngs::OsRng, RngCore};

use darkfi::util::serial::{SerialDecodable, SerialEncodable};

pub type PrivmsgId = u64;

pub const MAXIMUM_LENGTH_OF_MESSAGE: usize = 1024;
pub const MAXIMUM_LENGTH_OF_NICKNAME: usize = 32;

#[derive(Debug, Clone, SerialEncodable, SerialDecodable, Eq, PartialEq)]
pub struct Privmsg {
    pub id: PrivmsgId,
    pub nickname: String,
    pub target: String,
    pub message: String,
    pub timestamp: i64,
    pub term: u64,
}

impl Privmsg {
    pub fn new(nickname: &str, target: &str, message: &str, term: u64) -> Self {
        let id = OsRng.next_u64();
        let timestamp = Utc::now().timestamp();
        Self {
            id,
            nickname: nickname.to_string(),
            target: target.to_string(),
            message: message.to_string(),
            timestamp,
            term,
        }
    }
}

impl std::string::ToString for Privmsg {
    fn to_string(&self) -> String {
        format!(":{}!anon@dark.fi PRIVMSG {} :{}\r\n", self.nickname, self.target, self.message)
    }
}
