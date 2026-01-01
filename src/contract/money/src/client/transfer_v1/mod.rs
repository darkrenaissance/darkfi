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

use darkfi::{zk::ProvingKey, zkas::ZkBinary, ClientFailed, Result};
use darkfi_sdk::{
    crypto::{pasta_prelude::*, Blind, FuncId, Keypair, MerkleTree, PublicKey},
    pasta::pallas,
};
use rand::{prelude::SliceRandom, rngs::OsRng};
use tracing::{debug, error};

use crate::{
    client::OwnCoin,
    error::MoneyError,
    model::{MoneyTransferParamsV1, TokenId},
};

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
        error!(target: "contract::money::client::transfer::select_coins", "Not enough value to build tx inputs");
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
/// * `output_spend_hook: Optional contract spend hook to use in
///   the output, not applicable to the change
/// * `output_user_data: Optional user data to use in the output,
///   not applicable to the change
/// * `mint_zkbin`: `Mint_V1` zkas circuit ZkBinary
/// * `mint_pk`: Proving key for the `Mint_V1` zk circuit
/// * `burn_zkbin`: `Burn_V1` zkas circuit ZkBinary
/// * `burn_pk`: Proving key for the `Burn_V1` zk circuit
/// * `half_split`: Flag indicating to split the output coin into
///   two equal halves.
///
/// Returns a tuple of:
///
/// * The actual call data
/// * Secret values such as blinds
/// * A list of the spent coins
#[allow(clippy::too_many_arguments)]
pub fn make_transfer_call(
    keypair: Keypair,
    recipient: PublicKey,
    value: u64,
    token_id: TokenId,
    coins: Vec<OwnCoin>,
    tree: MerkleTree,
    output_spend_hook: Option<FuncId>,
    output_user_data: Option<pallas::Base>,
    mint_zkbin: ZkBinary,
    mint_pk: ProvingKey,
    burn_zkbin: ZkBinary,
    burn_pk: ProvingKey,
    half_split: bool,
) -> Result<(MoneyTransferParamsV1, TransferCallSecrets, Vec<OwnCoin>)> {
    debug!(target: "contract::money::client::transfer", "Building Money::TransferV1 contract call");
    if value == 0 {
        return Err(ClientFailed::InvalidAmount(value).into())
    }

    // Using integer division via `half_split` causes the evaluation of `1 / 2` which is equal to
    // 0. This would cause us to send two outputs of 0 value which is not what we want.
    if half_split && value == 1 {
        return Err(ClientFailed::InvalidAmount(value).into())
    }

    if token_id.inner() == pallas::Base::ZERO {
        return Err(ClientFailed::InvalidTokenId(token_id.to_string()).into())
    }

    if coins.is_empty() {
        return Err(ClientFailed::VerifyError(MoneyError::TransferMissingInputs.to_string()).into())
    }

    // Ensure the coins given to us are all of the same token ID.
    // The money contract base transfer doesn't allow conversions.
    for coin in &coins {
        if coin.note.token_id != token_id {
            return Err(ClientFailed::InvalidTokenId(coin.note.token_id.to_string()).into())
        }
    }

    let mut inputs = vec![];
    let mut outputs = vec![];

    let (spent_coins, change_value) = select_coins(coins, value)?;
    if spent_coins.is_empty() {
        error!(target: "contract::money::client::transfer", "Error: No coins selected");
        return Err(ClientFailed::VerifyError(MoneyError::TransferMissingInputs.to_string()).into())
    }

    for coin in spent_coins.iter() {
        let input = TransferCallInput {
            coin: coin.clone(),
            merkle_path: tree.witness(coin.leaf_position, 0).unwrap(),
            user_data_blind: Blind::random(&mut OsRng),
        };

        inputs.push(input);
    }

    // Check if we should split the output into two equal halves
    if half_split {
        // Cumpute each half value. If the value is odd,
        // the remainder(1) will be appended to the second half.
        let mut half = value / 2;

        // Add the first half, if its not zero
        if half != 0 {
            outputs.push(TransferCallOutput {
                public_key: recipient,
                value: half,
                token_id,
                spend_hook: output_spend_hook.unwrap_or(FuncId::none()),
                user_data: output_user_data.unwrap_or(pallas::Base::ZERO),
                blind: Blind::random(&mut OsRng),
            });
        }

        // Append the remainder and add the second half
        half += value % 2;
        outputs.push(TransferCallOutput {
            public_key: recipient,
            value: half,
            token_id,
            spend_hook: output_spend_hook.unwrap_or(FuncId::none()),
            user_data: output_user_data.unwrap_or(pallas::Base::ZERO),
            blind: Blind::random(&mut OsRng),
        });
    } else {
        outputs.push(TransferCallOutput {
            public_key: recipient,
            value,
            token_id,
            spend_hook: output_spend_hook.unwrap_or(FuncId::none()),
            user_data: output_user_data.unwrap_or(pallas::Base::ZERO),
            blind: Blind::random(&mut OsRng),
        });
    }

    if change_value > 0 {
        outputs.push(TransferCallOutput {
            public_key: keypair.public,
            value: change_value,
            token_id,
            spend_hook: FuncId::none(),
            user_data: pallas::Base::ZERO,
            blind: Blind::random(&mut OsRng),
        });
    }

    // Shuffle the outputs
    outputs.shuffle(&mut OsRng);

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
