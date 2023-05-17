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
    crypto::{Coin, MerklePosition, Nullifier, SecretKey, TokenId},
    pasta::pallas,
};
use darkfi_serial::{SerialDecodable, SerialEncodable};

/// `Money::TransferV1` API
pub mod transfer_v1;

/// `Money::OtcSwapV1` API
pub mod swap_v1;

/// `Money::GenesisMintV1` API
pub mod genesis_mint_v1;

/// `Money::MintV1` API
pub mod mint_v1;

/// `Money::FreezeV1` API
pub mod freeze_v1;

/// `Money::StakeV1` API
pub mod stake_v1;

/// `Money::UnstakeV1` API
pub mod unstake_v1;

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
pub const MONEY_COINS_COL_COIN_BLIND: &str = "coin_blind";
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

/// `MoneyNote` holds the inner attributes of a `Coin`.
#[derive(Debug, Clone, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct MoneyNote {
    /// Serial number of the coin, used for the nullifier
    pub serial: pallas::Base,
    /// Value of the coin
    pub value: u64,
    /// Token ID of the coin
    pub token_id: TokenId,
    /// Spend hook used for protocol-owned liquidity.
    /// Specifies which contract owns this coin.
    pub spend_hook: pallas::Base,
    /// User data used by protocol when spend hook is enabled
    pub user_data: pallas::Base,
    /// Blinding factor for the coin bulla
    pub coin_blind: pallas::Base,
    /// Blinding factor for the value pedersen commitment
    pub value_blind: pallas::Scalar,
    /// Blinding factor for the token ID pedersen commitment
    pub token_blind: pallas::Scalar,
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
    pub leaf_position: MerklePosition,
}
