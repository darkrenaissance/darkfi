/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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
    time::UNIX_EPOCH,
};

use crypto_box::PublicKey;
use darkfi::{Error::ParseFailed, Result};
use darkfi_sdk::{crypto::pasta_prelude::PrimeField, pasta::pallas};
use log::info;

use crate::{
    crypto::rln::{closest_epoch, RlnIdentity},
    irc::{IrcChannel, IrcContact},
};

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

pub fn list_configured_contacts(
    data: &toml::Value,
) -> Result<HashMap<String, (PublicKey, crypto_box::SecretKey)>> {
    let mut ret = HashMap::new();

    let Some(table) = data.as_table() else { return Err(ParseFailed("TOML not a map")) };
    let Some(contacts) = table.get("contact") else { return Ok(ret) };
    let Some(contacts) = contacts.as_table() else {
        return Err(ParseFailed("`contact` not a map"))
    };

    for (name, items) in contacts {
        let Some(public_str) = items.get("dm_chacha_public") else {
            return Err(ParseFailed("Invalid contact configuration dm_chacha_public missing"))
        };

        let Some(public_str) = public_str.as_str() else {
            return Err(ParseFailed("dm_chacha_public not a string"))
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

        // Parse the secret key for that specific contact
        let Some(my_secret) = items.get("my_dm_chacha_secret") else {
            return Err(ParseFailed("Invalid contact configuration my_dm_chacha_secret missing. \
            You can generate a keypair with: 'darkirc --gen-chacha-keypair' and then add that keypair to your config toml file."))
        };

        let Some(my_secret_str) = my_secret.as_str() else {
            return Err(ParseFailed("my_dm_chacha_secret not a string"))
        };

        let Ok(my_secret_bytes) = bs58::decode(my_secret_str).into_vec() else {
            return Err(ParseFailed("my_dm_chacha_secret not valid base58"))
        };

        if my_secret_bytes.len() != 32 {
            return Err(ParseFailed("my_dm_chacha_secret not 32 bytes long"))
        }

        let my_secret_bytes: [u8; 32] = my_secret_bytes.try_into().unwrap();

        let my_secret = crypto_box::SecretKey::from(my_secret_bytes);

        ret.insert(name.to_string(), (public, my_secret));
    }

    Ok(ret)
}

/// Parse configured contacts from a TOML map.
///
/// ```toml
/// [contact."anon"]
/// dm_chacha_public = "7CkVuFgwTUpJn5Sv67Q3fyEDpa28yrSeL5Hg2GqQ4jfM"
/// my_dm_chacha_secret = "A3mLrq4aW9UkFVY4zCfR2aLdEEWVUdH4u8v4o2dgi4kC"
/// ```
#[allow(clippy::type_complexity)]
pub fn parse_configured_contacts(data: &toml::Value) -> Result<HashMap<String, IrcContact>> {
    let mut ret = HashMap::new();

    let contacts = list_configured_contacts(data)?;
    if contacts.is_empty() {
        return Ok(ret);
    }

    for (name, (public, my_secret)) in contacts {
        let saltbox: Arc<crypto_box::ChaChaBox> =
            Arc::new(crypto_box::ChaChaBox::new(&public, &my_secret));
        let self_saltbox: Arc<crypto_box::ChaChaBox> =
            Arc::new(crypto_box::ChaChaBox::new(&my_secret.public_key(), &my_secret));

        if ret.contains_key(&name) {
            return Err(ParseFailed("Duplicate contact found"))
        }

        info!("Instantiated ChaChaBox for contact \"{}\"", name);
        ret.insert(name.to_string(), IrcContact { saltbox, self_saltbox });
    }

    Ok(ret)
}

/// Parse configured RLN identity from a TOML map.
///
/// ```toml
/// [rln]
/// nullifier = "6EGKCm3FdSK3fySbjY19pxG49aB34poXhaepsW5NMxFB"
/// trapdoor = "dCbf5fD2w3K9eYHA2ppgio3ui12tSMZXnEGm8dHS5x6"
/// user_message_limit = 100
/// ```
pub fn parse_rln_identity(data: &toml::Value) -> Result<Option<RlnIdentity>> {
    let Some(table) = data.as_table() else { return Err(ParseFailed("TOML not a map")) };
    let Some(rlninfo) = table.get("rln") else { return Ok(None) };

    let Some(nullifier) = rlninfo.get("nullifier") else {
        return Err(ParseFailed("RLN identity nullifier missing"))
    };

    let Some(trapdoor) = rlninfo.get("trapdoor") else {
        return Err(ParseFailed("RLN identity trapdoor missing"))
    };

    let Some(msglimit) = rlninfo.get("user_message_limit") else {
        return Err(ParseFailed("RLN user message limit missing"))
    };

    // Decode
    let identity_nullifier = if let Some(nullifier) = nullifier.as_str() {
        let Ok(nullifier_bytes) = bs58::decode(nullifier).into_vec() else {
            return Err(ParseFailed("RLN nullifier not valid base58"))
        };

        if nullifier_bytes.len() != 32 {
            return Err(ParseFailed("RLN nullifier not 32 bytes long"))
        }

        let Some(identity_nullifier) =
            pallas::Base::from_repr(nullifier_bytes.try_into().unwrap()).into()
        else {
            return Err(ParseFailed("RLN nullifier not a pallas base field element"))
        };

        identity_nullifier
    } else {
        return Err(ParseFailed("RLN nullifier not a string"))
    };

    let identity_trapdoor = if let Some(trapdoor) = trapdoor.as_str() {
        let Ok(trapdoor_bytes) = bs58::decode(trapdoor).into_vec() else {
            return Err(ParseFailed("RLN trapdoor not valid base58"))
        };

        if trapdoor_bytes.len() != 32 {
            return Err(ParseFailed("RLN trapdoor not 32 bytes long"))
        }

        let Some(identity_trapdoor) =
            pallas::Base::from_repr(trapdoor_bytes.try_into().unwrap()).into()
        else {
            return Err(ParseFailed("RLN trapdoor not a pallas base field element"))
        };

        identity_trapdoor
    } else {
        return Err(ParseFailed("RLN trapdoor not a string"))
    };

    let user_message_limit = if let Some(msglimit) = msglimit.as_float() {
        msglimit as u64
    } else {
        return Err(ParseFailed("RLN user message limit not a number"))
    };

    Ok(Some(RlnIdentity {
        nullifier: identity_nullifier,
        trapdoor: identity_trapdoor,
        user_message_limit,
        // TODO: FIXME: We should probably keep track of these rather than
        // resetting here
        message_id: 1,
        last_epoch: closest_epoch(UNIX_EPOCH.elapsed().unwrap().as_secs()),
    }))
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
