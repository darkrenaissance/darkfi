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

use darkfi_serial::{SerialDecodable, SerialEncodable};
use pasta_curves::pallas;

use super::{
    keypair::{PublicKey, SecretKey},
    util::poseidon_hash,
};

/// Contract ID used to reference smart contracts on the ledger.
#[repr(C)]
#[derive(Debug, Copy, Clone, SerialEncodable, SerialDecodable)]
pub struct ContractId(pallas::Base);

/// Derive a ContractId given a secret deploy key.
pub fn derive_contract_id(deploy_key: SecretKey) -> ContractId {
    let public_key = PublicKey::from_secret(deploy_key);
    let (x, y) = public_key.xy();
    let hash = poseidon_hash::<2>([x, y]);
    ContractId(hash)
}
