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
use darkfi::{zk::ProvingKey, zkas::ZkBinary, ClientFailed, Result};
use darkfi_sdk::{
    crypto::{pasta_prelude::*, Keypair, MerkleTree, PublicKey, TokenId},
    pasta::pallas,
};
use log::{debug, error};
use rand::rngs::OsRng;

use crate::{client::OwnCoin, model::MoneyTransferParamsV1};

mod builder;
pub use builder::{
    TransferCallBuilder, TransferCallClearInput, TransferCallInput, TransferCallOutput,
    TransferCallSecrets,
};

pub(crate) mod proof;

/// Select coins from `coins` of at least `min_value` in total.
/// Different strategies can be used. This function uses the dumb strategy
/// of selecting coins until we reach `min_value`.
pub fn select_coins(coins: Vec<OwnCoin>, min_value: u64) -> Result<(Vec<OwnCoin>, u64)> {
    let mut total_value = 0;
    let mut selected = vec![];

    for coin in coins {
        if total_value >= min_value {
            break
        }

        total_value += coin.note.value;
        selected.push(coin);
    }

    if total_value < min_value {
        error!("Not enough value to build tx inputs");
        return Err(ClientFailed::NotEnoughValue(total_value).into())
    }

    let change_value = total_value - min_value;

    Ok((selected, change_value))
}

/// Make a simple anonymous transfer call.
///
/// * `keypair`: Caller's keypair
/// * `recipient`: Recipient's public key
/// * `value`: Amount that we want to send to the recipient
/// * `token_id`: Token ID that we want to send to the recipient
/// * `coins`: Set of `OwnCoin` we're given to use in this builder
/// * `tree`: Merkle tree of coins used to create inclusion proofs
/// * `mint_zkbin`: `Mint_V1` zkas circuit ZkBinary
/// * `mint_pk`: Proving key for the `Mint_V1` zk circuit
/// * `burn_zkbin`: `Burn_V1` zkas circuit ZkBinary
/// * `burn_pk`: Proving key for the `Burn_V1` zk circuit
///
/// Returns a tuple of:
///
/// * The actual call data
/// * Secret values such as blinds
/// * A list of the spent coins
pub fn make_transfer_call(
    keypair: Keypair,
    recipient: PublicKey,
    value: u64,
    token_id: TokenId,
    coins: Vec<OwnCoin>,
    tree: MerkleTree,
    mint_zkbin: ZkBinary,
    mint_pk: ProvingKey,
    burn_zkbin: ZkBinary,
    burn_pk: ProvingKey,
) -> Result<(MoneyTransferParamsV1, TransferCallSecrets, Vec<OwnCoin>)> {
    debug!("Building Money::TransferV1 contract call");
    assert_ne!(value, 0);
    assert_ne!(token_id.inner(), pallas::Base::ZERO);
    assert!(!coins.is_empty());

    // Ensure the coins given to us are all of the same token ID.
    // The money contract base transfer doesn't allow conversions.
    for coin in &coins {
        assert_eq!(token_id, coin.note.token_id);
    }

    let mut inputs = vec![];
    let mut outputs = vec![];

    let (spent_coins, change_value) = select_coins(coins, value)?;

    for coin in spent_coins.iter() {
        let leaf_position = coin.leaf_position;
        let merkle_path = tree.witness(leaf_position, 0).unwrap();

        let input = TransferCallInput {
            leaf_position,
            merkle_path,
            secret: coin.secret,
            note: coin.note.clone(),
            user_data_blind: pallas::Base::random(&mut OsRng),
        };

        inputs.push(input);
    }
    debug!("Selected inputs");

    outputs.push(TransferCallOutput {
        value,
        token_id,
        public_key: recipient,
        spend_hook: pallas::Base::ZERO,
        user_data: pallas::Base::ZERO,
    });

    if change_value > 0 {
        outputs.push(TransferCallOutput {
            value: change_value,
            token_id,
            public_key: keypair.public,
            spend_hook: pallas::Base::ZERO,
            user_data: pallas::Base::ZERO,
        });
    }

    assert!(!inputs.is_empty());

    let xfer_builder = TransferCallBuilder {
        clear_inputs: vec![],
        inputs,
        outputs,
        mint_zkbin,
        mint_pk,
        burn_zkbin,
        burn_pk,
    };

    let (params, secrets) = xfer_builder.build()?;

    Ok((params, secrets, spent_coins))
}
