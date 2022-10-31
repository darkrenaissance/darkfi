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

pub struct ContractDeploy {
    /// Public address of the contract, derived from the deploy key.
    pub address: pallas::Base,
    /// Public key of the contract, derived from the deploy key.
    /// Used for signatures and authorizations, as well as deriving the
    /// contract's address.
    pub public: PublicKey,
    /// Compiled smart contract wasm binary to be executed in the wasm vm runtime.
    pub wasm_binary: Vec<u8>,
    /// Compiled zkas circuits used by the smart contract provers and verifiers.
    pub circuits: Vec<Vec<u8>>, // XXX: TODO: FIXME: The namespace of the zkas circuit should be in the bin
}
