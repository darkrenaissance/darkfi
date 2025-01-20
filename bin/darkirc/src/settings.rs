/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use crypto_box::{ChaChaBox, PublicKey};
use darkfi::{Error::ParseFailed, Result};
use log::info;

use crate::irc::{IrcChannel, IrcContact};

/// Parse configured autojoin channels from a TOML map.
///
/// ```toml
/// autojoin = ["#dev", "#memes"]
/// ```
pub fn parse_autojoin_channels(data: &toml::Value) -> Result<Vec<String>> {
    let mut ret = vec![];

    let Some(autojoin) = data.get("autojoin") else { return Ok(ret) };
    let Some(autojoin) = autojoin.as_array() else {
        return Err(ParseFailed("autojoin not an array"))
    };

    for item in autojoin {
        let Some(channel) = item.as_str() else {
            return Err(ParseFailed("autojoin channel not a string"))
        };

        if !channel.starts_with('#') {
            return Err(ParseFailed("autojoin channel not a valid channel"))
        }

        if ret.contains(&channel.to_string()) {
            return Err(ParseFailed("Duplicate autojoin channel found"))
        }

        ret.push(channel.to_string());
    }

    Ok(ret)
}

/// Parse a DM secret key from a TOML map.
///
/// ```toml
/// [crypto]
/// dm_chacha_secret = "7CkVuFgwTUpJn5Sv67Q3fyEDpa28yrSeL5Hg2GqQ4jfM"
/// ```
fn parse_dm_chacha_secret(data: &toml::Value) -> Result<Option<crypto_box::SecretKey>> {
    let Some(table) = data.as_table() else { return Err(ParseFailed("TOML not a map")) };
    let Some(crypto) = table.get("crypto") else { return Ok(None) };
    let Some(crypto) = crypto.as_table() else { return Err(ParseFailed("`crypto` not a map")) };

    if !crypto.contains_key("dm_chacha_secret") {
        return Ok(None)
    }

    let Some(secret_str) = crypto["dm_chacha_secret"].as_str() else {
        return Err(ParseFailed("dm_chacha_secret not a string"))
    };

    let Ok(secret_bytes) = bs58::decode(secret_str).into_vec() else {
        return Err(ParseFailed("dm_chacha_secret not valid base58"))
    };

    if secret_bytes.len() != 32 {
        return Err(ParseFailed("dm_chacha_secret not 32 bytes long"))
    }

    let secret_bytes: [u8; 32] = secret_bytes.try_into().unwrap();

    Ok(Some(crypto_box::SecretKey::from(secret_bytes)))
}

pub fn list_configured_contacts(data: &toml::Value) -> Result<HashMap<String, PublicKey>> {
    let mut ret = HashMap::new();

    let Some(table) = data.as_table() else { return Err(ParseFailed("TOML not a map")) };
    let Some(contacts) = table.get("contact") else { return Ok(ret) };
    let Some(contacts) = contacts.as_table() else {
        return Err(ParseFailed("`contact` not a map"))
    };

    for (name, items) in contacts {
        let Some(public_str) = items.get("dm_chacha_public") else {
            return Err(ParseFailed("Invalid contact configuration"))
        };

        let Some(public_str) = public_str.as_str() else {
            return Err(ParseFailed("Invalid contact configuration"))
        };

        let Ok(public_bytes) = bs58::decode(public_str).into_vec() else {
            return Err(ParseFailed("Invalid base58 for contact pubkey"))
        };

        if public_bytes.len() != 32 {
            return Err(ParseFailed("Invalid contact pubkey (not 32 bytes)"))
        }

        let public_bytes: [u8; 32] = public_bytes.try_into().unwrap();

        let public = crypto_box::PublicKey::from(public_bytes);

        if ret.contains_key(name) {
            return Err(ParseFailed("Duplicate contact found"))
        }

        ret.insert(name.to_string(), public);
    }

    Ok(ret)
}

/// Parse configured contacts from a TOML map.
/// If contacts exist and our secret key is valid, also return its saltbox.
///
/// ```toml
/// [contact."anon"]
/// dm_chacha_public = "7CkVuFgwTUpJn5Sv67Q3fyEDpa28yrSeL5Hg2GqQ4jfM"
/// ```
#[allow(clippy::type_complexity)]
pub fn parse_configured_contacts(
    data: &toml::Value,
) -> Result<(HashMap<String, IrcContact>, Option<Arc<ChaChaBox>>)> {
    let mut ret = HashMap::new();

    let contacts = list_configured_contacts(data)?;
    if contacts.is_empty() {
        return Ok((ret, None));
    }
    let Some(secret) = parse_dm_chacha_secret(data)? else {
        return Err(ParseFailed("You have specified some contacts but you did not set up a valid chacha secret for yourself.  You can generate a keypair with: 'darkirc --gen-chacha-keypair' and then add that keypair to your config toml file."))
    };
    for (name, public) in contacts {
        let saltbox = Some(Arc::new(crypto_box::ChaChaBox::new(&public, &secret)));

        if ret.contains_key(&name) {
            return Err(ParseFailed("Duplicate contact found"))
        }

        info!("Instantiated ChaChaBox for contact \"{}\"", name);
        ret.insert(name.to_string(), IrcContact { saltbox });
    }

    Ok((ret, Some(Arc::new(crypto_box::ChaChaBox::new(&secret.public_key(), &secret)))))
}

/// Parse a TOML string for any configured channels and return
/// a map containing said configurations.
///
/// ```toml
/// [channel."#memes"]
/// secret = "7CkVuFgwTUpJn5Sv67Q3fyEDpa28yrSeL5Hg2GqQ4jfM"
/// topic = "Dank Memes"
/// ```
pub fn parse_configured_channels(data: &toml::Value) -> Result<HashMap<String, IrcChannel>> {
    let mut ret = HashMap::new();

    let Some(table) = data.as_table() else { return Err(ParseFailed("TOML not a map")) };
    let Some(chans) = table.get("channel") else { return Ok(ret) };
    let Some(chans) = chans.as_table() else { return Err(ParseFailed("`channel` not a map")) };

    for (name, items) in chans {
        let mut chan = IrcChannel { topic: String::new(), nicks: HashSet::new(), saltbox: None };

        if let Some(topic) = items.get("topic") {
            if let Some(topic) = topic.as_str() {
                info!("Found configured topic for {}: {}", name, topic);
                chan.topic = topic.to_string();
            } else {
                return Err(ParseFailed("Channel topic not a string"))
            }
        }

        if let Some(secret) = items.get("secret") {
            if let Some(secret) = secret.as_str() {
                let Ok(secret_bytes) = bs58::decode(secret).into_vec() else {
                    return Err(ParseFailed("Channel secret not valid base58"))
                };

                if secret_bytes.len() != 32 {
                    return Err(ParseFailed("Channel secret not 32 bytes long"))
                }

                let secret_bytes: [u8; 32] = secret_bytes.try_into().unwrap();
                let secret = crypto_box::SecretKey::from(secret_bytes);
                let public = secret.public_key();
                chan.saltbox = Some(Arc::new(crypto_box::ChaChaBox::new(&public, &secret)));
                info!("Configured NaCl box for channel {}", name);
            } else {
                return Err(ParseFailed("Channel secret not a string"))
            }
        }

        info!("Configured channel {}", name);
        ret.insert(name.to_string(), chan);
    }

    Ok(ret)
}
