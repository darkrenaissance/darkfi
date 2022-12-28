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
    settings::{ChannelInfo, ContactInfo},
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
pub fn encrypt(salt_box: &SalsaBox, plaintext: &str) -> String {
    let nonce = SalsaBox::generate_nonce(&mut OsRng);
    let mut ciphertext = salt_box.encrypt(&nonce, plaintext.as_bytes()).unwrap();

    let mut concat = vec![];
    concat.append(&mut nonce.as_slice().to_vec());
    concat.append(&mut ciphertext);

    bs58::encode(concat).into_string()
}

/// Decrypt PrivMsg target
pub fn decrypt_target(
    privmsg: &mut PrivMsgEvent,
    configured_chans: &HashMap<String, ChannelInfo>,
    configured_contacts: &HashMap<String, ContactInfo>,
    private_key: &Option<String>,
) {
    for (name, chan_info) in configured_chans {
        if !chan_info.joined {
            continue
        }

        let salt_box = chan_info.salt_box(name).clone();

        if let Some(salt_box) = salt_box {
            if try_decrypt(&salt_box, &privmsg.target).is_some() {
                privmsg.target = name.clone();
                return
            }
        }
    }

    if private_key.is_none() {
        return
    }

    for (name, contact_info) in configured_contacts {
        let salt_box = contact_info.salt_box(private_key.as_ref().unwrap(), name).clone();

        if let Some(salt_box) = salt_box {
            if try_decrypt(&salt_box, &privmsg.target).is_some() {
                privmsg.target = name.clone();
                return
            }
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

    privmsg.nick = decrypted_nick.unwrap();
    privmsg.msg = decrypted_msg.unwrap();
}

/// Encrypt PrivMsg
pub fn encrypt_privmsg(salt_box: &SalsaBox, privmsg: &mut PrivMsgEvent) {
    privmsg.nick = encrypt(salt_box, &privmsg.nick);
    privmsg.target = encrypt(salt_box, &privmsg.target);
    privmsg.msg = encrypt(salt_box, &privmsg.msg);
}
