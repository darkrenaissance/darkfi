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

use darkfi_sdk::{crypto::PublicKey, pasta::pallas};
use darkfi_serial::{SerialDecodable, SerialEncodable};

use crate::net;

/// This struct represents a tuple of the form:
/// (`public_key`, `node_address`, `last_slot_seen`,`slot_quarantined`)
#[derive(Debug, Clone, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct Participant {
    /// Node public key
    pub public_key: PublicKey,
    /// Node current epoch competing coins public inputs
    pub coins: Vec<Vec<Vec<pallas::Base>>>,
}

impl Participant {
    pub fn new(public_key: PublicKey, coins: Vec<Vec<Vec<pallas::Base>>>) -> Self {
        Self { public_key, coins }
    }
}

impl net::Message for Participant {
    fn name() -> &'static str {
        "participant"
    }
}
