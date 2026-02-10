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

use darkfi_sdk::{
    crypto::{
        note::AeadEncryptedNote, pasta_prelude::PrimeField, poseidon_hash, BaseBlind, FuncId,
        MerkleNode, PublicKey, ScalarBlind,
    },
    error::ContractError,
    pasta::pallas,
};
use darkfi_serial::{SerialDecodable, SerialEncodable};

#[cfg(feature = "client")]
use darkfi_serial::async_trait;

/// Nullifier definitions
pub mod nullifier;
pub use nullifier::Nullifier;

/// Token ID definitions and methods
pub mod token_id;
pub use token_id::{TokenId, DARK_TOKEN_ID};

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

use core::str::FromStr;
darkfi_sdk::fp_from_bs58!(Coin);
darkfi_sdk::fp_to_bs58!(Coin);
darkfi_sdk::ty_from_fp!(Coin);

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
// ANCHOR: coin-attributes
pub struct CoinAttributes {
    pub public_key: PublicKey,
    pub value: u64,
    pub token_id: TokenId,
    pub spend_hook: FuncId,
    pub user_data: pallas::Base,
    /// Simultaneously blinds the coin and ensures uniqueness
    pub blind: BaseBlind,
}
// ANCHOR_END: coin-attributes

impl CoinAttributes {
    pub fn to_coin(&self) -> Coin {
        let (pub_x, pub_y) = self.public_key.xy();
        let coin = poseidon_hash([
            pub_x,
            pub_y,
            pallas::Base::from(self.value),
            self.token_id.inner(),
            self.spend_hook.inner(),
            self.user_data,
            self.blind.inner(),
        ]);
        Coin(coin)
    }
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct TokenAttributes {
    pub auth_parent: FuncId,
    pub user_data: pallas::Base,
    pub blind: BaseBlind,
}

impl TokenAttributes {
    pub fn to_token_id(&self) -> TokenId {
        let token_id =
            poseidon_hash([self.auth_parent.inner(), self.user_data, self.blind.inner()]);
        TokenId::from(token_id)
    }
}

#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
// ANCHOR: money-clear-input
/// A contract call's clear input
pub struct ClearInput {
    /// Input's value (amount)
    pub value: u64,
    /// Input's token ID
    pub token_id: TokenId,
    /// Blinding factor for `value`
    pub value_blind: ScalarBlind,
    /// Blinding factor for `token_id`
    pub token_blind: BaseBlind,
    /// Public key for the signature
    pub signature_public: PublicKey,
}
// ANCHOR_END: money-clear-input

#[derive(Clone, Debug, PartialEq, SerialEncodable, SerialDecodable)]
// ANCHOR: money-input
/// A contract call's anonymous input
pub struct Input {
    /// Pedersen commitment for the input's value
    pub value_commit: pallas::Point,
    /// Commitment for the input's token ID
    pub token_commit: pallas::Base,
    /// Revealed nullifier
    pub nullifier: Nullifier,
    /// Revealed Merkle root
    pub merkle_root: MerkleNode,
    /// Encrypted user data field. An encrypted commitment to arbitrary data.
    /// When spend hook is nonzero, then this field may be used to pass data
    /// to the invoked contract.
    pub user_data_enc: pallas::Base,
    /// Public key for the signature
    pub signature_public: PublicKey,
}
// ANCHOR_END: money-input

#[derive(Clone, Debug, PartialEq, SerialEncodable, SerialDecodable)]
// ANCHOR: money-output
/// A contract call's anonymous output
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
// ANCHOR_END: money-output

/// Parameters for `Money::Fee`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct MoneyFeeParamsV1 {
    /// Anonymous input
    pub input: Input,
    /// Anonymous outputs
    pub output: Output,
    /// Fee value blind
    pub fee_value_blind: ScalarBlind,
    /// Token ID blind
    pub token_blind: BaseBlind,
}

/// State update for `Money::Fee`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct MoneyFeeUpdateV1 {
    /// Revealed nullifier
    pub nullifier: Nullifier,
    /// Minted coin
    pub coin: Coin,
    /// Block height the fee was verified against
    pub height: u32,
    /// Height accumulated fee paid
    pub fee: u64,
}

#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
// ANCHOR: money-params
/// Parameters for `Money::Transfer` and `Money::OtcSwap`
pub struct MoneyTransferParamsV1 {
    /// Anonymous inputs
    pub inputs: Vec<Input>,
    /// Anonymous outputs
    pub outputs: Vec<Output>,
}
// ANCHOR_END: money-params

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
    /// Anonymous outputs
    pub outputs: Vec<Output>,
}

/// State update for `Money::GenesisMint`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct MoneyGenesisMintUpdateV1 {
    /// The newly minted coins
    pub coins: Vec<Coin>,
}

/// Parameters for `Money::TokenMint`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct MoneyTokenMintParamsV1 {
    /// The newly minted coin
    pub coin: Coin,
}

/// State update for `Money::TokenMint`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct MoneyTokenMintUpdateV1 {
    /// The newly minted coin
    pub coin: Coin,
}

/// Parameters for `Money::AuthTokenMint`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct MoneyAuthTokenMintParamsV1 {
    pub token_id: TokenId,
    pub enc_note: AeadEncryptedNote,
    pub mint_pubkey: PublicKey,
}

/// State update for `Money::AuthTokenMint`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct MoneyAuthTokenMintUpdateV1 {}

/// Parameters for `Money::AuthTokenFreeze`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct MoneyAuthTokenFreezeParamsV1 {
    /// Mint authority public key
    ///
    /// We use this to derive the token ID and verify the signature.
    pub mint_public: PublicKey,
    pub token_id: TokenId,
}

/// State update for `Money::AuthTokenFreeze`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct MoneyAuthTokenFreezeUpdateV1 {
    pub token_id: TokenId,
}

/// Parameters for `Money::BurnV1`
///
/// Burns (destroys) coins, removing value from circulation permanently.
/// The call has inputs but no outputs; the value committed in the inputs
/// is destroyed. All inputs must use the same token commitment.
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct MoneyBurnParamsV1 {
    /// Anonymous inputs
    pub inputs: Vec<Input>,
}

/// State update for `Money::BurnV1`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct MoneyBurnUpdateV1 {
    /// Revealed nullifiers from the burned coins
    pub nullifiers: Vec<Nullifier>,
}

/// Parameters for `Money::PoWReward`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct MoneyPoWRewardParamsV1 {
    /// Clear input
    pub input: ClearInput,
    /// Anonymous output
    pub output: Output,
}

/// State update for `Money::PoWReward`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct MoneyPoWRewardUpdateV1 {
    /// The newly minted coin
    pub coin: Coin,
    /// Block height the call was verified against
    pub height: u32,
}
