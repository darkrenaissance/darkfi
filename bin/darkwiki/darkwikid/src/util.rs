/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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
    collections::HashMap,
    fs::{create_dir_all, read_dir},
    path::{Path, PathBuf},
};

use darkfi_serial::{deserialize, serialize};
use dryoc::{
    classic::crypto_secretbox::{crypto_secretbox_easy, crypto_secretbox_open_easy, Key, Nonce},
    constants::CRYPTO_SECRETBOX_MACBYTES,
    dryocbox::NewByteArray,
};
use log::{error, info, warn};
use unicode_segmentation::UnicodeSegmentation;

use darkfi::{util::path::expand_path, Error, Result};

use crate::{Args, EncryptedPatch, Patch};

/// Split a `&str` into a vector of each of its chars.
pub fn str_to_chars(s: &str) -> Vec<&str> {
    s.graphemes(true).collect::<Vec<&str>>()
}

/// Parse a base58 string for a `crypto_secretbox` secret.
fn parse_b58_secret(s: &str) -> Result<[u8; 32]> {
    match bs58::decode(s).into_vec() {
        Ok(v) => {
            if v.len() != 32 {
                return Err(Error::Custom("Secret is not 32 bytes long".to_string()))
            }

            Ok(v.try_into().unwrap())
        }
        Err(e) => Err(Error::Custom(format!("Unable to parse secret from base58: {}", e))),
    }
}

/// Parse a TOML string for configured workspaces and return an `HashMap`
/// of parsed data. Does not error on failures, just warns if something is
/// misconfigured.
pub fn parse_workspaces(toml_str: &str) -> HashMap<String, Key> {
    let mut ret = HashMap::new();

    let settings: Args = match toml::from_str(toml_str) {
        Ok(v) => v,
        Err(e) => {
            error!("Failed parsing TOML from string: {}", e);
            return ret
        }
    };

    for workspace in settings.workspace {
        let wrk: Vec<&str> = workspace.split(':').collect();
        if wrk.len() != 2 {
            warn!("Invalid workspace: {}", workspace);
            continue
        }

        let secret = match parse_b58_secret(wrk[1]) {
            Ok(v) => v,
            Err(e) => {
                warn!("Failed parsing secret for workspace {}: {}", wrk[0], e);
                continue
            }
        };

        let docs_path = match expand_path(&settings.docs) {
            Ok(v) => v,
            Err(e) => {
                warn!("Failed expanding docs path for workspace {}: {}", wrk[0], e);
                continue
            }
        };

        if let Err(e) = create_dir_all(docs_path.join(wrk[0])) {
            warn!("Failed creating directory for workspace {}: {}", wrk[0], e);
            continue
        }

        info!("Added parsed workspace: {}", wrk[0]);
        ret.insert(wrk[0].to_string(), secret);
    }

    ret
}

/// Encrypt a patch using a NaCl crypto_secretbox given a `Patch` and a `Key`.
pub fn encrypt_patch(patch: &Patch, key: &Key) -> Result<EncryptedPatch> {
    let nonce = Nonce::gen();
    let payload = serialize(patch);

    let mut ciphertext = vec![0u8; payload.len() + CRYPTO_SECRETBOX_MACBYTES];

    if let Err(e) = crypto_secretbox_easy(&mut ciphertext, &payload, &nonce, key) {
        error!("encrypt_patch: Failed encrypting patch: {}", e);
        return Err(Error::Custom(format!("Failed encrypting darkwiki patch: {}", e)))
    }

    Ok(EncryptedPatch { nonce, ciphertext })
}

/// Decrypt a patch using a NaCl crypto_secretbox given an `EncryptedPatch` and a `Key`.
pub fn decrypt_patch(patch: &EncryptedPatch, key: &Key) -> Result<Patch> {
    let nonce = &patch.nonce;
    let ciphertext = &patch.ciphertext;

    let mut decrypted = vec![0u8; ciphertext.len() - CRYPTO_SECRETBOX_MACBYTES];
    if let Err(e) = crypto_secretbox_open_easy(&mut decrypted, ciphertext, nonce, key) {
        error!("decrypt_patch: Failed decrypting patch: {}", e);
        return Err(Error::Custom(format!("Failed decrypting darkwiki patch: {}", e)))
    }

    Ok(deserialize(&decrypted)?)
}

/// TODO: DOCUMENT ME
/// FIXME: There's checking of file extensions here. Take care that the rest of the code
/// is robust against this attack.
pub fn get_docs_paths(files: &mut Vec<PathBuf>, path: &Path, parent: Option<&Path>) -> Result<()> {
    let docs = read_dir(path)?;
    let docs = docs.filter(|d| d.is_ok()).map(|d| d.unwrap().path()).collect::<Vec<PathBuf>>();

    for doc in docs {
        if let Some(f) = doc.file_name() {
            let filename = PathBuf::from(f);
            let filename = if let Some(parent) = parent { parent.join(filename) } else { filename };

            if doc.is_file() {
                if let Some(ext) = doc.extension() {
                    if ext == "md" || ext == "markdown" {
                        files.push(filename);
                    }
                }
            } else if doc.is_dir() {
                if f == ".log" {
                    continue
                }

                get_docs_paths(files, &doc, Some(&filename))?;
            }
        }
    }

    Ok(())
}

/// Hash a path and workspace, and encode with base58, providing an ID.
pub fn path_to_id(path: &str, workspace: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(path.as_bytes());
    hasher.update(workspace.as_bytes());
    bs58::encode(hasher.finalize().as_bytes()).into_string()
}
