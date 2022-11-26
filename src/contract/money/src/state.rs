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

use darkfi_sdk::{
    crypto::{
        pedersen::{ValueBlind, ValueCommit},
        Coin, MerkleNode, Nullifier, PublicKey, TokenId,
    },
    pasta::pallas,
};
use darkfi_serial::{SerialDecodable, SerialEncodable};

/// Inputs and outputs for a payment
#[derive(Debug, SerialEncodable, SerialDecodable)]
pub struct MoneyTransferParams {
    /// Clear inputs
    pub clear_inputs: Vec<ClearInput>,
    /// Anonymous inputs
    pub inputs: Vec<Input>,
    /// Anonymous outputs
    pub outputs: Vec<Output>,
}

/// State update produced by a payment
#[derive(Debug, SerialEncodable, SerialDecodable)]
pub struct MoneyTransferUpdate {
    /// Revealed nullifiers
    pub nullifiers: Vec<Nullifier>,
    /// Minted coins
    pub coins: Vec<Coin>,
}

/// A transaction's clear input
#[derive(Debug, SerialEncodable, SerialDecodable)]
pub struct ClearInput {
    /// Input's value (amount)
    pub value: u64,
    /// Input's token ID
    pub token_id: TokenId,
    /// Blinding factor for `value`
    pub value_blind: ValueBlind,
    /// Blinding factor for `token_id`
    pub token_blind: ValueBlind,
    /// Public key for the signature
    pub signature_public: PublicKey,
}

/// A transaction's anonymous input
#[derive(Debug, SerialEncodable, SerialDecodable)]
pub struct Input {
    /// Pedersen commitment for the input's value
    pub value_commit: ValueCommit,
    /// Pedersen commitment for the input's token ID
    pub token_commit: ValueCommit,
    /// Revealed nullifier
    pub nullifier: Nullifier,
    /// Revealed Merkle root
    pub merkle_root: MerkleNode,
	/// Allows composing this ZK proof to invoke other contracts TODO:add more details in DOCS
    pub spend_hook: pallas::Base,
    /// Data passed from this coin to the invoked contract TODO:add more details in DOCS
    pub user_data_enc: pallas::Base,
    /// Public key for the signature
    pub signature_public: PublicKey,
}

/// A transaction's anonymous output
#[derive(Debug, SerialEncodable, SerialDecodable)]
pub struct Output {
    /// Pedersen commitment for the output's value
    pub value_commit: ValueCommit,
    /// Pedersen commitment for the output's token ID
    pub token_commit: ValueCommit,
    /// Minted coin
    pub coin: pallas::Base,
    //pub coin: Coin,
    /// The encrypted note ciphertext
    pub ciphertext: Vec<u8>,
    /// The ephemeral public key
    pub ephem_public: PublicKey,
}
