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

//! Contract Client API
//!
//! This module implements the client-side API for this contract's interaction.
//! What we basically do here is implement an API that creates the necessary
//! structures and is able to export them to create a DarkFi transaction
//! object that can be broadcasted to the network when we want to make a
//! payment with some coins in our wallet.
//!
//! Note that this API does not involve any wallet interaction, but only takes
//! the necessary objects provided by the caller. This is intentional, so we
//! are able to abstract away any wallet interfaces to client implementations.

use std::hash::{Hash, Hasher};

use darkfi_sdk::{
    bridgetree,
    crypto::{
        pasta_prelude::{Field, PrimeField},
        poseidon_hash, BaseBlind, Blind, FuncId, ScalarBlind, SecretKey,
    },
    pasta::pallas,
};
use darkfi_serial::{async_trait, SerialDecodable, SerialEncodable};

use crate::model::{Coin, Nullifier, TokenId};

/// `Money::FeeV1` API
pub mod fee_v1;

/// `Money::GenesisMintV1` API
pub mod genesis_mint_v1;

/// `Money::PoWRewardV1` API
pub mod pow_reward_v1;

/// `Money::TransferV1` API
pub mod transfer_v1;

/// `Money::OtcSwapV1` API
pub mod swap_v1;

/// `Money::AuthTokenMintV1` API
pub mod auth_token_mint_v1;

/// `Money::AuthTokenFreezeV1` API
pub mod auth_token_freeze_v1;

/// `Money::TokenMintV1` API
pub mod token_mint_v1;

/// `Money::BurnV1` API
pub mod burn_v1;

/// `MoneyNote` holds the inner attributes of a `Coin`.
///
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
    pub spend_hook: FuncId,
    /// User data used by protocol when spend hook is enabled
    pub user_data: pallas::Base,
    /// Blinding factor for the coin
    pub coin_blind: BaseBlind,
    // TODO: look into removing these fields. We potentially don't need them [
    /// Blinding factor for the value pedersen commitment
    pub value_blind: ScalarBlind,
    /// Blinding factor for the token ID pedersen commitment
    pub token_blind: BaseBlind,
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
    /// Coin's leaf position in the Merkle tree of coins
    pub leaf_position: bridgetree::Position,
}

impl OwnCoin {
    /// Derive the [`Nullifier`] for this [`OwnCoin`]
    pub fn nullifier(&self) -> Nullifier {
        Nullifier::from(poseidon_hash([self.secret.inner(), self.coin.inner()]))
    }
}

impl Hash for OwnCoin {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.coin.inner().to_repr().hash(state);
    }
}

pub fn compute_remainder_blind(
    input_blinds: &[ScalarBlind],
    output_blinds: &[ScalarBlind],
) -> ScalarBlind {
    let mut total = pallas::Scalar::ZERO;

    for input_blind in input_blinds {
        total += input_blind.inner();
    }

    for output_blind in output_blinds {
        total -= output_blind.inner();
    }

    Blind(total)
}
