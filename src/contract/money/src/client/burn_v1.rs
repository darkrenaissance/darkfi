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

use darkfi::{
    zk::{Proof, ProvingKey},
    zkas::ZkBinary,
    ClientFailed, Result,
};
use darkfi_sdk::crypto::{BaseBlind, Blind, MerkleTree, ScalarBlind, SecretKey};
use rand::rngs::OsRng;
use tracing::debug;

use crate::{
    client::{
        transfer_v1::{proof::create_transfer_burn_proof, TransferCallInput},
        OwnCoin,
    },
    error::MoneyError,
    model::{Input, MoneyBurnParamsV1},
};

/// Struct holding necessary information to build a `Money::BurnV1`
/// contract call.
pub struct BurnCallBuilder {
    /// Anonymous inputs
    pub inputs: Vec<TransferCallInput>,
    /// `Burn_V1` zkas circuit ZkBinary
    pub burn_zkbin: ZkBinary,
    /// Proving key for the `Burn_V1` zk circuit
    pub burn_pk: ProvingKey,
}

impl BurnCallBuilder {
    pub fn build(self) -> Result<(MoneyBurnParamsV1, BurnCallDebris)> {
        debug!(target: "contract::money::client::burn::build", "Building Money::BurnV1 contract call");
        if self.inputs.is_empty() {
            return Err(ClientFailed::VerifyError(MoneyError::BurnMissingInputs.to_string()).into())
        }

        let mut params = MoneyBurnParamsV1 { inputs: vec![] };
        let mut signature_secrets = vec![];
        let mut proofs = vec![];

        let token_blind = BaseBlind::random(&mut OsRng);
        let mut input_value_blinds = vec![];

        debug!(target: "contract::money::client::burn::build", "Building anonymous inputs");
        for (i, input) in self.inputs.iter().enumerate() {
            let value_blind = Blind::random(&mut OsRng);
            input_value_blinds.push(value_blind);

            let signature_secret = SecretKey::random(&mut OsRng);
            signature_secrets.push(signature_secret);

            debug!(target: "contract::money::client::burn::build", "Creating burn proof for input {i}");
            let (proof, public_inputs) = create_transfer_burn_proof(
                &self.burn_zkbin,
                &self.burn_pk,
                input,
                value_blind,
                token_blind,
                signature_secret,
            )?;

            params.inputs.push(Input {
                value_commit: public_inputs.value_commit,
                token_commit: public_inputs.token_commit,
                nullifier: public_inputs.nullifier,
                merkle_root: public_inputs.merkle_root,
                user_data_enc: public_inputs.user_data_enc,
                signature_public: public_inputs.signature_public,
                intra_tx: false,
            });

            proofs.push(proof);
        }

        let secrets = BurnCallDebris { proofs, signature_secrets, input_value_blinds, token_blind };
        Ok((params, secrets))
    }
}

pub struct BurnCallDebris {
    /// The ZK proofs created in this builder
    pub proofs: Vec<Proof>,
    /// The ephemeral secret keys created for signing
    pub signature_secrets: Vec<SecretKey>,
    /// The value blinds created for each input
    pub input_value_blinds: Vec<ScalarBlind>,
    /// The token blind used for all inputs
    pub token_blind: BaseBlind,
}

/// Make a simple burn call to permanently destroy coins.
///
/// * `coins`: Set of `OwnCoin` we're given to burn in this call
/// * `tree`: Merkle tree of coins used to create inclusion proofs
/// * `burn_zkbin`: `Burn_V1` zkas circuit ZkBinary
/// * `burn_pk`: Proving key for the `Burn_V1` zk circuit
///
/// Returns a tuple of:
///
/// * The actual call data
/// * Secret values such as blinds
/// * A list of the spent coins
pub fn make_burn_call(
    coins: Vec<OwnCoin>,
    tree: MerkleTree,
    burn_zkbin: ZkBinary,
    burn_pk: ProvingKey,
) -> Result<(MoneyBurnParamsV1, BurnCallDebris, Vec<OwnCoin>)> {
    debug!(target: "contract::money::client::burn", "Building Money::BurnV1 contract call");

    if coins.is_empty() {
        return Err(ClientFailed::VerifyError(MoneyError::BurnMissingInputs.to_string()).into())
    }

    // Ensure the coins given to us are all of the same token ID.
    let token_id = coins[0].note.token_id;
    for coin in &coins {
        if coin.note.token_id != token_id {
            return Err(ClientFailed::InvalidTokenId(coin.note.token_id.to_string()).into())
        }
    }

    let mut inputs = vec![];
    for coin in coins.iter() {
        let input = TransferCallInput {
            coin: coin.clone(),
            merkle_path: tree.witness(coin.leaf_position, 0).unwrap(),
            user_data_blind: Blind::random(&mut OsRng),
        };

        inputs.push(input);
    }

    let burn_builder = BurnCallBuilder { inputs, burn_zkbin, burn_pk };

    let (params, secrets) = burn_builder.build()?;

    Ok((params, secrets, coins))
}
