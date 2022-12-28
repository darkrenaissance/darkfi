/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use chrono::Utc;
use darkfi_serial::{SerialDecodable, SerialEncodable};
use rand::{rngs::OsRng, RngCore};

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
