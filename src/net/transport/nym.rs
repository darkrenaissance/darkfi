/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use std::{io, time::Duration};

use rand::{rngs::OsRng, RngCore};
use url::Url;

use crate::util::encoding::base32;

/// Unique, randomly-generated per-connection ID that's used to
/// identify which connection a message belongs to.
// TODO: remove this when implemented properly
#[allow(dead_code)]
#[derive(Clone, Eq, PartialEq, Hash)]
struct ConnectionId([u8; 32]);

impl ConnectionId {
    fn _generate() -> Self {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        Self(bytes)
    }

    fn _from_bytes(bytes: &[u8]) -> Self {
        let mut id = [0u8; 32];
        id[..].copy_from_slice(&bytes[0..32]);
        ConnectionId(id)
    }
}

impl std::fmt::Debug for ConnectionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", base32::encode(false, &self.0).to_ascii_lowercase())
    }
}

/// Nym Dialer implementation
#[derive(Debug, Clone)]
pub struct NymDialer;

impl NymDialer {
    /// Instantiate a new [`NymDialer`] object
    pub(crate) async fn new() -> io::Result<Self> {
        Ok(Self {})
    }

    pub(crate) async fn _do_dial(
        &self,
        _endpoint: Url, // Recipient
        _timeout: Option<Duration>,
    ) -> io::Result<()> {
        let _id = ConnectionId::_generate();

        Ok(())
    }
}
