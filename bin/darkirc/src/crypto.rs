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

use std::{collections::HashMap, fmt};

use crypto_box::{
    aead::{Aead, AeadCore},
    SalsaBox,
};
use rand::rngs::OsRng;

use crate::{
    privmsg::PrivMsgEvent,
    settings::{ChannelInfo, ContactInfo, MAXIMUM_LENGTH_OF_NICK_CHAN_CNT},
};

#[derive(serde::Serialize)]
pub struct KeyPair {
    pub private_key: String,
    pub public_key: String,
}

impl fmt::Display for KeyPair {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Public key: {}\nPrivate key: {}", self.public_key, self.private_key)
    }
}

/// The format we're using is nonce+ciphertext, where nonce is 24 bytes.
fn try_decrypt(salt_box: &SalsaBox, ciphertext: &str) -> Option<String> {
    let bytes = match bs58::decode(ciphertext).into_vec() {
        Ok(v) => v,
        Err(_) => return None,
    };

    if bytes.len() < 25 {
        return None
    }

    // Try extracting the nonce
    let nonce = match bytes[0..24].try_into() {
        Ok(v) => v,
        Err(_) => return None,
    };

    // Take the remaining ciphertext
    let message = &bytes[24..];

    // Try decrypting the message
    match salt_box.decrypt(nonce, message) {
        Ok(v) => Some(String::from_utf8_lossy(&v).to_string()),
        Err(_) => None,
    }
}

/// The format we're using is nonce+ciphertext, where nonce is 24 bytes.
pub fn encrypt(salt_box: &SalsaBox, plaintext: &[u8]) -> String {
    let nonce = SalsaBox::generate_nonce(&mut OsRng);
    let mut ciphertext = salt_box.encrypt(&nonce, plaintext).unwrap();

    let mut concat = vec![];
    concat.append(&mut nonce.as_slice().to_vec());
    concat.append(&mut ciphertext);

    bs58::encode(concat).into_string()
}

pub fn decrypt_target(
    contact: &mut String,
    privmsg: &mut PrivMsgEvent,
    configured_chans: HashMap<String, ChannelInfo>,
    configured_contacts: HashMap<String, ContactInfo>,
) {
    for chan_name in configured_chans.keys() {
        let chan_info = configured_chans.get(chan_name).unwrap();
        if !chan_info.joined {
            continue
        }

        let salt_box = chan_info.salt_box.clone();

        if let Some(salt_box) = salt_box {
            let decrypted_target = try_decrypt(&salt_box, &privmsg.target);
            if decrypted_target.is_none() {
                continue
            }

            let target =
                String::from_utf8_lossy(&unpad(decrypted_target.unwrap().into())).to_string();
            if *chan_name == target {
                privmsg.target = target;
                return
            }
        }
    }

    for cnt_name in configured_contacts.keys() {
        let cnt_info = configured_contacts.get(cnt_name).unwrap();

        let salt_box = cnt_info.salt_box.clone();
        if let Some(salt_box) = salt_box {
            let decrypted_target = try_decrypt(&salt_box, &privmsg.target);
            if decrypted_target.is_none() {
                continue
            }

            let target =
                String::from_utf8_lossy(&unpad(decrypted_target.unwrap().into())).to_string();
            privmsg.target = target;
            *contact = cnt_name.into();
            return
        }
    }
}

/// Decrypt PrivMsg nickname and message
pub fn decrypt_privmsg(salt_box: &SalsaBox, privmsg: &mut PrivMsgEvent) {
    let decrypted_nick = try_decrypt(salt_box, &privmsg.nick);
    let decrypted_msg = try_decrypt(salt_box, &privmsg.msg);

    if decrypted_nick.is_none() && decrypted_msg.is_none() {
        return
    }

    privmsg.nick = String::from_utf8_lossy(&unpad(decrypted_nick.unwrap().into())).to_string();
    privmsg.msg = decrypted_msg.unwrap();
}

/// Encrypt PrivMsg
pub fn encrypt_privmsg(salt_box: &SalsaBox, privmsg: &mut PrivMsgEvent) {
    privmsg.nick = encrypt(salt_box, &pad(privmsg.nick.clone().into()));
    privmsg.target = encrypt(salt_box, &pad(privmsg.target.clone().into()));
    privmsg.msg = encrypt(salt_box, privmsg.msg.as_bytes());
}

fn pad(data: Vec<u8>) -> Vec<u8> {
    if data.len() == MAXIMUM_LENGTH_OF_NICK_CHAN_CNT {
        return data
    }

    assert!(data.len() < MAXIMUM_LENGTH_OF_NICK_CHAN_CNT);
    let padding = vec![0u8; MAXIMUM_LENGTH_OF_NICK_CHAN_CNT - data.len()];

    let mut data = data.clone();
    data.extend_from_slice(&padding);
    data
}

fn unpad(data: Vec<u8>) -> Vec<u8> {
    assert!(data.len() == MAXIMUM_LENGTH_OF_NICK_CHAN_CNT);
    match data.iter().position(|&x| x == 0u8) {
        Some(idx) => data[..idx].to_vec(),
        None => data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pad_unpad() {
        let nick = String::from("terry-davis");

        let padded = pad(nick.clone().into());
        assert!(padded.len() == 32);
        assert_eq!(nick, String::from_utf8_lossy(&unpad(padded)));

        let nick = String::from("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        let padded = pad(nick.clone().into());
        assert_eq!(nick, String::from_utf8_lossy(&padded));
        assert_eq!(nick, String::from_utf8_lossy(&unpad(padded)));
    }
}
