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
    crypto::{
        ecvrf::VrfProof, note::AeadEncryptedNote, pasta_prelude::PrimeField, poseidon_hash,
        MerkleNode, Nullifier, PublicKey, TokenId,
    },
    error::ContractError,
    pasta::pallas,
};
use darkfi_serial::{SerialDecodable, SerialEncodable};

#[cfg(feature = "client")]
use darkfi_serial::async_trait;

/// A `Coin` represented in the Money state
#[derive(Debug, Clone, Copy, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct Coin(pallas::Base);

impl Coin {
    /// Reference the raw inner base field element
    pub fn inner(&self) -> pallas::Base {
        self.0
    }

    /// Create a `Coin` object from given bytes, erroring if the input
    /// bytes are noncanonical.
    pub fn from_bytes(x: [u8; 32]) -> Result<Self, ContractError> {
        match pallas::Base::from_repr(x).into() {
            Some(v) => Ok(Self(v)),
            None => {
                Err(ContractError::IoError("Failed to instantiate Coin from bytes".to_string()))
            }
        }
    }

    /// Convert the `Coin` type into 32 raw bytes
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_repr()
    }
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CoinParams {
    pub public_key: PublicKey,
    pub value: u64,
    pub token_id: TokenId,
    pub serial: pallas::Base,
    pub spend_hook: pallas::Base,
    pub user_data: pallas::Base,
}

impl CoinParams {
    pub fn to_coin(&self) -> Coin {
        let (pub_x, pub_y) = self.public_key.xy();
        let coin = poseidon_hash([
            pub_x,
            pub_y,
            pallas::Base::from(self.value),
            self.token_id.inner(),
            self.serial,
            self.spend_hook,
            self.user_data,
        ]);
        Coin(coin)
    }
}

use core::str::FromStr;
darkfi_sdk::fp_from_bs58!(Coin);
darkfi_sdk::fp_to_bs58!(Coin);
darkfi_sdk::ty_from_fp!(Coin);

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
    pub token_blind: pallas::Base,
    /// Public key for the signature
    pub signature_public: PublicKey,
}

/// A contract call's anonymous input
#[derive(Clone, Debug, PartialEq, SerialEncodable, SerialDecodable)]
pub struct Input {
    /// Pedersen commitment for the input's value
    pub value_commit: pallas::Point,
    /// Commitment for the input's token ID
    pub token_commit: pallas::Base,
    /// Revealed nullifier
    pub nullifier: Nullifier,
    /// Revealed Merkle root
    pub merkle_root: MerkleNode,
    /// Spend hook used to invoke other contracts.
    /// If this value is nonzero then the subsequent contract call in the tx
    /// must have this value as its ID.
    pub spend_hook: pallas::Base,
    /// Encrypted user data field. An encrypted commitment to arbitrary data.
    /// When spend hook is set (it is nonzero), then this field may be used
    /// to pass data to the invoked contract.
    pub user_data_enc: pallas::Base,
    /// Public key for the signature
    pub signature_public: PublicKey,
}

/// Anonymous input for consensus contract calls
#[derive(Clone, Debug, PartialEq, SerialEncodable, SerialDecodable)]
pub struct ConsensusInput {
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

/// A contract call's anonymous output
#[derive(Clone, Debug, PartialEq, SerialEncodable, SerialDecodable)]
pub struct Output {
    /// Pedersen commitment for the output's value
    pub value_commit: pallas::Point,
    /// Commitment for the output's token ID
    pub token_commit: pallas::Base,
    /// Minted coin
    pub coin: Coin,
    /// AEAD encrypted note
    pub note: AeadEncryptedNote,
}

/// A consensus contract call's anonymous output
#[derive(Clone, Debug, PartialEq, SerialEncodable, SerialDecodable)]
pub struct ConsensusOutput {
    /// Pedersen commitment for the output's value
    pub value_commit: pallas::Point,
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

/// Parameters for `Money::GenesisMint`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct MoneyGenesisMintParamsV1 {
    /// Clear input
    pub input: ClearInput,
    /// Anonymous output
    pub output: Output,
}

/// State update for `Money::GenesisMint`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct MoneyGenesisMintUpdateV1 {
    /// The newly minted coin
    pub coin: Coin,
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

/// Parameters for `Money::PoWReward`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct MoneyPoWRewardParamsV1 {
    /// Clear input
    pub input: ClearInput,
    /// Anonymous output
    pub output: Output,
    /// Extending fork last proposal/block hash
    pub fork_hash: blake3::Hash,
    /// Extending fork second to last proposal/block hash
    pub fork_previous_hash: blake3::Hash,
    /// VRF proof for block rank calculation
    pub vrf_proof: VrfProof,
}

/// State update for `Money::PoWReward`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct MoneyPoWRewardUpdateV1 {
    /// The newly minted coin
    pub coin: Coin,
}

/// Parameters for `Money::Stake`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
// ANCHOR: MoneyStakeParams
pub struct MoneyStakeParamsV1 {
    /// Blinding factor for `token_id`
    pub token_blind: pallas::Base,
    /// Anonymous input
    pub input: Input,
}
// ANCHOR_END: MoneyStakeParams

/// State update for `Money::Stake`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
// ANCHOR: MoneyStakeUpdate
pub struct MoneyStakeUpdateV1 {
    /// Revealed nullifier
    pub nullifier: Nullifier,
}
// ANCHOR_END: MoneyStakeUpdate

/// Parameters for `Money::Unstake`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
// ANCHOR: MoneyUnstakeParams
pub struct MoneyUnstakeParamsV1 {
    /// Burnt token revealed info
    pub input: ConsensusInput,
    /// Anonymous output
    pub output: Output,
}
// ANCHOR_END: MoneyUnstakeParams

/// State update for `Money::Unstake`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
// ANCHOR: MoneyUnstakeUpdate
pub struct MoneyUnstakeUpdateV1 {
    /// The newly minted coin
    pub coin: Coin,
}
// ANCHOR_END: MoneyUnstakeUpdate

/// Parameters for `Consensus::Stake`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
// ANCHOR: ConsensusStakeParams
pub struct ConsensusStakeParamsV1 {
    /// Burnt token revealed info
    pub input: Input,
    /// Anonymous output
    pub output: ConsensusOutput,
}
// ANCHOR_END: ConsensusStakeParams

/// State update for `Consensus::Stake`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
// ANCHOR: ConsensusStakeUpdate
pub struct ConsensusStakeUpdateV1 {
    /// The newly minted coin
    pub coin: Coin,
}
// ANCHOR_END: ConsensusStakeUpdate

/// Parameters for `Consensus::UnstakeRequest`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
// ANCHOR: ConsensusUnstakeReqParams
pub struct ConsensusUnstakeReqParamsV1 {
    pub input: ConsensusInput,
    pub output: ConsensusOutput,
}
// ANCHOR_END: ConsensusUnstakeReqParams

/// Parameters for `Consensus::Unstake`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
// ANCHOR: ConsensusUnstakeParams
pub struct ConsensusUnstakeParamsV1 {
    /// Anonymous input
    pub input: ConsensusInput,
}
// ANCHOR_END: ConsensusUnstakeParams

/// State update for `Consensus::Unstake`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
// ANCHOR: ConsensusUnstakeUpdate
pub struct ConsensusUnstakeUpdateV1 {
    /// Revealed nullifier
    pub nullifier: Nullifier,
}
// ANCHOR_END: ConsensusUnstakeUpdate
