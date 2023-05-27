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

//! This API is crufty. Please rework it into something nice to read and nice to use.

use darkfi::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{
        pasta_prelude::*, pedersen_commitment_u64, poseidon_hash, Coin, PublicKey, TokenId,
        CONSENSUS_CONTRACT_ID,
    },
    pasta::pallas,
};
use rand::rngs::OsRng;

use crate::model::ZERO;

pub struct TransactionBuilderOutputInfo {
    pub value: u64,
    pub token_id: TokenId,
    pub public_key: PublicKey,
}

pub struct ConsensusMintRevealed {
    pub epoch: pallas::Base,
    pub coin: Coin,
    pub value_commit: pallas::Point,
}

impl ConsensusMintRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let valcom_coords = self.value_commit.to_affine().coordinates().unwrap();

        // NOTE: It's important to keep these in the same order
        // as the `constrain_instance` calls in the zkas code.
        vec![self.epoch, self.coin.inner(), *valcom_coords.x(), *valcom_coords.y()]
    }
}

pub fn create_consensus_mint_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    epoch: u64,
    output: &TransactionBuilderOutputInfo,
    value_blind: pallas::Scalar,
    serial: pallas::Base,
    coin_blind: pallas::Base,
) -> Result<(Proof, ConsensusMintRevealed)> {
    let epoch_pallas = pallas::Base::from(epoch);
    let value_pallas = pallas::Base::from(output.value);
    let value_commit = pedersen_commitment_u64(output.value, value_blind);
    let (pub_x, pub_y) = output.public_key.xy();

    let coin =
        Coin::from(poseidon_hash([pub_x, pub_y, value_pallas, epoch_pallas, serial, coin_blind]));

    let public_inputs = ConsensusMintRevealed { epoch: epoch_pallas, coin, value_commit };

    let prover_witnesses = vec![
        Witness::Base(Value::known(pub_x)),
        Witness::Base(Value::known(pub_y)),
        Witness::Base(Value::known(value_pallas)),
        Witness::Base(Value::known(epoch_pallas)),
        Witness::Base(Value::known(serial)),
        Witness::Base(Value::known(coin_blind)),
        Witness::Scalar(Value::known(value_blind)),
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin.clone());
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
