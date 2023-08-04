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

use darkfi::Result;

use crate::model::{Session, SlotInfo};

pub fn make_node_id(node_name: &String) -> Result<String> {
    let mut id = hex::encode(node_name);
    id.insert_str(0, "NODE");
    Ok(id)
}

pub fn make_session_id(node_id: &str, session: &Session) -> Result<String> {
    let mut num = 0_u64;

    let session_chars = match session {
        Session::Inbound => vec!['i', 'n'],
        Session::Outbound => vec!['o', 'u', 't'],
        Session::Manual => vec!['m', 'a', 'n'],
        Session::Offline => vec!['o', 'f', 'f'],
    };

    for i in session_chars {
        num += i as u64
    }

    for i in node_id.chars() {
        num += i as u64
    }

    let mut id = hex::encode(num.to_ne_bytes());
    id.insert_str(0, "SESSION");
    Ok(id)
}

pub fn make_info_id(id: &u64) -> Result<String> {
    let mut id = hex::encode(id.to_ne_bytes());
    id.insert_str(0, "INFO");
    Ok(id)
}

pub fn make_empty_id(node_id: &str, session: &Session, count: u64) -> Result<String> {
    let count = count * 2;

    let mut num = 0_u64;

    let id = match session {
        Session::Inbound => {
            let session_chars = vec!['i', 'n'];
            for i in session_chars {
                num += i as u64
            }
            for i in node_id.chars() {
                num += i as u64
            }
            num += count;
            let mut id = hex::encode(num.to_ne_bytes());
            id.insert_str(0, "EMPTYIN");
            id
        }
        Session::Outbound => {
            let session_chars = vec!['o', 'u', 't'];
            for i in session_chars {
                num += i as u64
            }
            for i in node_id.chars() {
                num += i as u64
            }
            num += count;
            let mut id = hex::encode(num.to_ne_bytes());
            id.insert_str(0, "EMPTYOUT");
            id
        }
        Session::Manual => {
            let session_chars = vec!['m', 'a', 'n'];
            for i in session_chars {
                num += i as u64
            }
            for i in node_id.chars() {
                num += i as u64
            }
            num += count;
            let mut id = hex::encode(num.to_ne_bytes());
            id.insert_str(0, "EMPTYMAN");
            id
        }
        Session::Offline => {
            let session_chars = vec!['o', 'f', 'f'];
            for i in session_chars {
                num += i as u64
            }
            for i in node_id.chars() {
                num += i as u64
            }
            num += count;
            let mut id = hex::encode(num.to_ne_bytes());
            id.insert_str(0, "EMPTYOFF");
            id
        }
    };

    Ok(id)
}

// TODO: Rename to is empty slot.
pub fn is_empty_session(connects: &[SlotInfo]) -> bool {
    return connects.iter().all(|conn| conn.is_empty)
}
