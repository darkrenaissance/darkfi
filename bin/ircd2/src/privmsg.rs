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

use std::io;

use darkfi_serial::{Decodable, Encodable, ReadExt, SerialDecodable, SerialEncodable};

#[derive(SerialEncodable, SerialDecodable, Clone)]
pub struct PrivMsgEvent {
    pub nick: String,
    pub msg: String,
    pub target: String,
}

#[derive(Clone)]
pub enum EventAction {
    PrivMsg(PrivMsgEvent),
}

impl std::string::ToString for PrivMsgEvent {
    fn to_string(&self) -> String {
        format!(":{}!anon@dark.fi PRIVMSG {} :{}\r\n", self.nick, self.target, self.msg)
    }
}

impl Encodable for EventAction {
    fn encode<S: io::Write>(&self, mut s: S) -> core::result::Result<usize, io::Error> {
        match self {
            Self::PrivMsg(event) => {
                let mut len = 0;
                len += 0u8.encode(&mut s)?;
                len += event.encode(s)?;
                Ok(len)
            }
        }
    }
}

impl Decodable for EventAction {
    fn decode<D: io::Read>(mut d: D) -> core::result::Result<Self, io::Error> {
        let type_id = d.read_u8()?;
        match type_id {
            0 => Ok(Self::PrivMsg(PrivMsgEvent::decode(d)?)),
            _ => Err(io::Error::new(io::ErrorKind::Other, "Bad type ID byte for Event")),
        }
    }
}
