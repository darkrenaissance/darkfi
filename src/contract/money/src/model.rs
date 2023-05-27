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

use darkfi_sdk::{
    crypto::{note::AeadEncryptedNote, Coin, MerkleNode, Nullifier, PublicKey, TokenId},
    pasta::pallas,
};
use darkfi_serial::{SerialDecodable, SerialEncodable};

/// A contract call's clear input
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct ClearInput {
    /// Input's value (amount)
    pub value: u64,
    /// Input's token ID
    pub token_id: TokenId,
    /// Blinding factor for `value`
    pub value_blind: pallas::Scalar,
    /// Blinding factor for `token_id`
    pub token_blind: pallas::Scalar,
    /// Public key for the signature
    pub signature_public: PublicKey,
}

/// A contract call's anonymous input
#[derive(Clone, Debug, PartialEq, SerialEncodable, SerialDecodable)]
pub struct Input {
    /// Pedersen commitment for the input's value
    pub value_commit: pallas::Point,
    /// Pedersen commitment for the input's token ID
    pub token_commit: pallas::Point,
    /// Revealed nullifier
    pub nullifier: Nullifier,
    /// Revealed Merkle root
    pub merkle_root: MerkleNode,
    /// Spend hook used to invoke other contracts.
    /// If this value is nonzero then the subsequent contract call in the tx
    /// must have this value as its ID.
    pub spend_hook: pallas::Base,
    /// Encrypted user data field. An encrypted commitment to arbitrary data.
    /// When spend hook is set (it is nonzero), then this field may be user
    /// to pass data to the invoked contract.
    pub user_data_enc: pallas::Base,
    /// Public key for the signature
    pub signature_public: PublicKey,
}

/// Anonymous input for staking contract calls
#[derive(Clone, Debug, PartialEq, SerialEncodable, SerialDecodable)]
pub struct StakeInput {
    /// Blinding factor for `token_id`
    pub token_blind: pallas::Scalar,
    /// Pedersen commitment for the staked coin's value
    pub value_commit: pallas::Point,
    /// Revealed nullifier
    pub nullifier: Nullifier,
    /// Revealed Merkle root
    pub merkle_root: MerkleNode,
    /// Public key for the signature
    pub signature_public: PublicKey,
}

impl PartialEq<StakeInput> for Input {
    fn eq(&self, other: &StakeInput) -> bool {
        self.value_commit == other.value_commit &&
            self.nullifier == other.nullifier &&
            self.merkle_root == other.merkle_root &&
            self.signature_public == other.signature_public
    }
}

/// Anonymous input for unstaking contract calls
#[derive(Clone, Debug, PartialEq, SerialEncodable, SerialDecodable)]
pub struct UnstakeInput {
    /// Epoch the coin was minted
    pub epoch: u64,
    /// Pedersen commitment for the staked coin's value
    pub value_commit: pallas::Point,
    /// Revealed nullifier
    pub nullifier: Nullifier,
    /// Revealed Merkle root
    pub merkle_root: MerkleNode,
    /// Public key for the signature
    pub signature_public: PublicKey,
}

impl PartialEq<StakeInput> for UnstakeInput {
    fn eq(&self, other: &StakeInput) -> bool {
        self.value_commit == other.value_commit &&
            self.nullifier == other.nullifier &&
            self.merkle_root == other.merkle_root &&
            self.signature_public == other.signature_public
    }
}

/// A contract call's anonymous output
#[derive(Clone, Debug, PartialEq, SerialEncodable, SerialDecodable)]
pub struct Output {
    /// Pedersen commitment for the output's value
    pub value_commit: pallas::Point,
    /// Pedersen commitment for the output's token ID
    pub token_commit: pallas::Point,
    /// Minted coin
    pub coin: Coin,
    /// AEAD encrypted note
    pub note: AeadEncryptedNote,
}

/// Parameters for `Money::Transfer` and `Money::OtcSwap`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct MoneyTransferParamsV1 {
    /// Clear inputs
    pub clear_inputs: Vec<ClearInput>,
    /// Anonymous inputs
    pub inputs: Vec<Input>,
    /// Anonymous outputs
    pub outputs: Vec<Output>,
}

/// State update for `Money::Transfer` and `Money::OtcSwap`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct MoneyTransferUpdateV1 {
    /// Revealed nullifiers
    pub nullifiers: Vec<Nullifier>,
    /// Minted coins
    pub coins: Vec<Coin>,
}

/// Parameters for `Money::TokenMint`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct MoneyTokenMintParamsV1 {
    /// Clear input
    pub input: ClearInput,
    /// Anonymous output
    pub output: Output,
}

/// State update for `Money::TokenMint`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct MoneyTokenMintUpdateV1 {
    /// The newly minted coin
    pub coin: Coin,
}

/// Parameters for `Money::TokenFreeze`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct MoneyTokenFreezeParamsV1 {
    /// Mint authority public key
    ///
    /// We use this to derive the token ID and verify the signature.
    pub signature_public: PublicKey,
}

/// State update for `Money::TokenFreeze`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct MoneyTokenFreezeUpdateV1 {
    /// Mint authority public key
    pub signature_public: PublicKey,
}

/// Parameters for `Money::Stake`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct MoneyStakeParamsV1 {
    /// Blinding factor for `token_id`
    pub token_blind: pallas::Scalar,
    /// Anonymous input
    pub input: Input,
}

/// State update for `Money::Stake`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct MoneyStakeUpdateV1 {
    /// Revealed nullifier
    pub nullifier: Nullifier,
}

/// Parameters for `Money::Unstake`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct MoneyUnstakeParamsV1 {
    /// Burnt token revealed info
    pub input: StakeInput,
    /// Spend hook used to invoke other contracts.
    /// If this value is nonzero then the subsequent contract call in the tx
    /// must have this value as its ID.
    pub spend_hook: pallas::Base,
    /// Encrypted user data field. An encrypted commitment to arbitrary data.
    /// When spend hook is set (it is nonzero), then this field may be user
    /// to pass data to the invoked contract.
    pub user_data_enc: pallas::Base,
    /// Anonymous output
    pub output: Output,
}

/// State update for `Money::Unstake`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct MoneyUnstakeUpdateV1 {
    /// The newly minted coin
    pub coin: Coin,
}

/// Parameters for `Consensus::Stake`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct ConsensusStakeParamsV1 {
    /// Burnt token revealed info
    pub input: StakeInput,
    /// Anonymous output
    pub output: Output,
}

/// State update for `Consensus::Stake`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct ConsensusStakeUpdateV1 {
    /// The newly minted coin
    pub coin: Coin,
}

/// Parameters for `Consensus::Unstake`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct ConsensusUnstakeParamsV1 {
    /// Anonymous input
    pub input: UnstakeInput,
}

/// State update for `Consensus::Unstake`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct ConsensusUnstakeUpdateV1 {
    /// Revealed nullifier
    pub nullifier: Nullifier,
}
