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
    fs::{File, OpenOptions},
    os::unix::prelude::OpenOptionsExt,
    path::Path,
};

use crypto_box::aead::Aead;
use log::{debug, error};

use darkfi::{Error, Result};
use rand::{distributions::Alphanumeric, rngs::OsRng, Rng};

use crate::{
    error::{TaudError, TaudResult},
    task_info::{TaskEvent, TaskInfo},
};

pub fn set_event(task_info: &mut TaskInfo, action: &str, author: &str, content: &str) {
    debug!(target: "tau", "TaskInfo::set_event()");
    if !content.is_empty() {
        task_info.events.push(TaskEvent::new(action.into(), author.into(), content.into()));
    }
}

pub fn pipe_write<P: AsRef<Path>>(path: P) -> Result<File> {
    OpenOptions::new().append(true).custom_flags(libc::O_NONBLOCK).open(path).map_err(Error::from)
}

pub fn gen_id(len: usize) -> String {
    OsRng.sample_iter(&Alphanumeric).take(len).map(char::from).collect()
}

pub fn check_write_access(write: Option<String>, password: Option<String>) -> TaudResult<bool> {
    let secret = if write.is_some() {
        let scrt = write.clone().unwrap();
        let bytes: [u8; 32] = bs58::decode(scrt)
            .into_vec()
            .map_err(|_| {
                Error::ParseFailed("Parse secret key failed, couldn't decode into vector of bytes")
            })?
            .try_into()
            .map_err(|_| Error::ParseFailed("Parse secret key failed"))?;
        crypto_box::SecretKey::from(bytes)
    } else {
        crypto_box::SecretKey::generate(&mut OsRng)
    };

    let public = secret.public_key();
    let chacha_box = crypto_box::ChaChaBox::new(&public, &secret);

    if password.is_some() {
        let bytes = match bs58::decode(password.clone().unwrap()).into_vec() {
            Ok(v) => v,
            Err(_) => return Err(TaudError::DecryptionError("Error decoding payload".to_string())),
        };

        if bytes.len() < 25 {
            return Err(TaudError::DecryptionError("Invalid bytes length".to_string()))
        }

        // Try extracting the nonce
        let nonce = bytes[0..24].into();

        // Take the remaining ciphertext
        let pswd = &bytes[24..];

        if chacha_box.decrypt(nonce, pswd).is_err() {
            error!(target: "taud", "You don't have write access");
            return Ok(false);
        };
    } else {
        error!(target: "taud", "You don't have write access");
        return Ok(false);
    };

    Ok(true)
}
