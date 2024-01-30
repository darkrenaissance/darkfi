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

//! This module implements the client-side API for this contract's interaction.
//! What we basically do here is implement an API that creates the necessary
//! structures and is able to export them to create a DarkFi transaction
//! object that can be broadcasted to the network when we want to make a
//! payment with some coins in our wallet.
//!
//! Note that this API does not involve any wallet interaction, but only takes
//! the necessary objects provided by the caller. This is intentional, so we
//! are able to abstract away any wallet interfaces to client implementations.

use darkfi_sdk::{
    bridgetree,
    crypto::{Nullifier, SecretKey, TokenId},
    pasta::pallas,
};
use darkfi_serial::{async_trait, SerialDecodable, SerialEncodable};

use crate::model::Coin;

/// `Money::FeeV1` API
pub mod fee_v1;

/// `Money::TransferV1` API
pub mod transfer_v1;

/// `Money::OtcSwapV1` API
pub mod swap_v1;

/// `Money::GenesisMintV1` API
pub mod genesis_mint_v1;

/// `Money::TokenMintV1` API
pub mod token_mint_v1;

/// `Money::TokenFreezeV1` API
pub mod token_freeze_v1;

/// `Money::PoWRewardV1` API
pub mod pow_reward_v1;

// Wallet SQL table constant names. These have to represent the `wallet.sql`
// SQL schema.
// TODO: They should also be prefixed with the contract ID to avoid collisions.
pub const MONEY_INFO_TABLE: &str = "money_info";
pub const MONEY_INFO_COL_LAST_SCANNED_SLOT: &str = "last_scanned_slot";

pub const MONEY_TREE_TABLE: &str = "money_tree";
pub const MONEY_TREE_COL_TREE: &str = "tree";

pub const MONEY_KEYS_TABLE: &str = "money_keys";
pub const MONEY_KEYS_COL_KEY_ID: &str = "key_id";
pub const MONEY_KEYS_COL_IS_DEFAULT: &str = "is_default";
pub const MONEY_KEYS_COL_PUBLIC: &str = "public";
pub const MONEY_KEYS_COL_SECRET: &str = "secret";

pub const MONEY_COINS_TABLE: &str = "money_coins";
pub const MONEY_COINS_COL_COIN: &str = "coin";
pub const MONEY_COINS_COL_IS_SPENT: &str = "is_spent";
pub const MONEY_COINS_COL_SERIAL: &str = "serial";
pub const MONEY_COINS_COL_VALUE: &str = "value";
pub const MONEY_COINS_COL_TOKEN_ID: &str = "token_id";
pub const MONEY_COINS_COL_SPEND_HOOK: &str = "spend_hook";
pub const MONEY_COINS_COL_USER_DATA: &str = "user_data";
pub const MONEY_COINS_COL_VALUE_BLIND: &str = "value_blind";
pub const MONEY_COINS_COL_TOKEN_BLIND: &str = "token_blind";
pub const MONEY_COINS_COL_SECRET: &str = "secret";
pub const MONEY_COINS_COL_NULLIFIER: &str = "nullifier";
pub const MONEY_COINS_COL_LEAF_POSITION: &str = "leaf_position";
pub const MONEY_COINS_COL_MEMO: &str = "memo";

pub const MONEY_TOKENS_TABLE: &str = "money_tokens";
pub const MONEY_TOKENS_COL_MINT_AUTHORITY: &str = "mint_authority";
pub const MONEY_TOKENS_COL_TOKEN_ID: &str = "token_id";
pub const MONEY_TOKENS_COL_IS_FROZEN: &str = "is_frozen";

pub const MONEY_ALIASES_TABLE: &str = "money_aliases";
pub const MONEY_ALIASES_COL_ALIAS: &str = "alias";
pub const MONEY_ALIASES_COL_TOKEN_ID: &str = "token_id";

/// `MoneyNote` holds the inner attributes of a `Coin`
/// It does not store the public key since it's encrypted for that key,
/// and so is not needed to infer the coin attributes.
/// All other coin attributes must be present.
#[derive(Debug, Clone, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct MoneyNote {
    /// Value of the coin
    pub value: u64,
    /// Token ID of the coin
    pub token_id: TokenId,
    /// Spend hook used for protocol-owned liquidity.
    /// Specifies which contract owns this coin.
    pub spend_hook: pallas::Base,
    /// User data used by protocol when spend hook is enabled
    pub user_data: pallas::Base,
    /// Blinding factor for the coin
    pub coin_blind: pallas::Base,
    // TODO: look into removing these fields. We potentially don't need them [
    /// Blinding factor for the value pedersen commitment
    pub value_blind: pallas::Scalar,
    /// Blinding factor for the token ID pedersen commitment
    pub token_blind: pallas::Base,
    // ] ^ the receiver is not interested in the value commit / token commits.
    // we just want to examine the coins in the outputs. The money::transfer() contract
    // should ensure everything else is correct.
    /// Attached memo (arbitrary data)
    pub memo: Vec<u8>,
}

/// `OwnCoin` is a representation of `Coin` with its respective metadata.
#[derive(Debug, Clone, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct OwnCoin {
    /// The coin hash
    pub coin: Coin,
    /// The attached `MoneyNote`
    pub note: MoneyNote,
    /// Coin's secret key
    pub secret: SecretKey,
    /// Coin's nullifier
    pub nullifier: Nullifier,
    /// Coin's leaf position in the Merkle tree of coins
    pub leaf_position: bridgetree::Position,
}

pub fn compute_remainder_blind(
    clear_inputs: &[crate::model::ClearInput],
    input_blinds: &[pallas::Scalar],
    output_blinds: &[pallas::Scalar],
) -> pallas::Scalar {
    let mut total = pallas::Scalar::zero();

    for input in clear_inputs {
        total += input.value_blind;
    }

    for input_blind in input_blinds {
        total += input_blind;
    }

    for output_blind in output_blinds {
        total -= output_blind;
    }

    total
}
