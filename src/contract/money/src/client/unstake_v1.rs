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
        note::AeadEncryptedNote, pasta_prelude::*, pedersen_commitment_base,
        pedersen_commitment_u64, poseidon_hash, Coin, MerkleNode, Nullifier, PublicKey, TokenId,
        DARK_TOKEN_ID,
    },
    pasta::pallas,
};
use log::{debug, info};
use rand::rngs::OsRng;

use crate::{
    client::{MoneyNote, OwnCoin},
    model::{MoneyUnstakeParamsV1, Output, StakeInput},
};

pub struct MoneyUnstakeCallDebris {
    pub params: MoneyUnstakeParamsV1,
    pub proofs: Vec<Proof>,
}

pub struct MoneyMintRevealed {
    pub coin: Coin,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Point,
}

impl MoneyMintRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let valcom_coords = self.value_commit.to_affine().coordinates().unwrap();
        let tokcom_coords = self.token_commit.to_affine().coordinates().unwrap();

        // NOTE: It's important to keep these in the same order
        // as the `constrain_instance` calls in the zkas code.
        vec![
            self.coin.inner(),
            *valcom_coords.x(),
            *valcom_coords.y(),
            *tokcom_coords.x(),
            *tokcom_coords.y(),
        ]
    }
}

pub struct TransactionBuilderOutputInfo {
    pub value: u64,
    pub token_id: TokenId,
    pub public_key: PublicKey,
}

/// Struct holding necessary information to build a `Money::UnstakeV1` contract call.
pub struct MoneyUnstakeCallBuilder {
    /// `OwnCoin` we're given to use in this builder
    pub coin: OwnCoin,
    /// Recipient's public key
    pub recipient: PublicKey,
    /// Blinding factor for value commitment
    pub value_blind: pallas::Scalar,
    /// Blinding factor for `token_id`
    pub token_blind: pallas::Scalar,
    /// Revealed nullifier
    pub nullifier: Nullifier,
    /// Revealed Merkle root
    pub merkle_root: MerkleNode,
    /// Public key for the signature
    pub signature_public: PublicKey,
    /// `Mint_V1` zkas circuit ZkBinary
    pub mint_zkbin: ZkBinary,
    /// Proving key for the `Mint_V1` zk circuit
    pub mint_pk: ProvingKey,
}

impl MoneyUnstakeCallBuilder {
    pub fn build(&self) -> Result<MoneyUnstakeCallDebris> {
        debug!("Building Money::UnstakeV1 contract call");
        assert!(self.coin.note.value != 0);
        assert!(self.coin.note.token_id == *DARK_TOKEN_ID);

        debug!("Building anonymous output");
        let output = TransactionBuilderOutputInfo {
            value: self.coin.note.value,
            token_id: self.coin.note.token_id,
            public_key: self.recipient,
        };
        debug!("Finished building output");

        let serial = pallas::Base::random(&mut OsRng);
        let spend_hook = pallas::Base::zero();
        let user_data_enc = pallas::Base::zero();
        let coin_blind = pallas::Base::random(&mut OsRng);

        info!("Creating unstake mint proof for output");
        let (proof, public_inputs) = create_unstake_mint_proof(
            &self.mint_zkbin,
            &self.mint_pk,
            &output,
            self.value_blind,
            self.token_blind,
            serial,
            spend_hook,
            user_data_enc,
            coin_blind,
        )?;

        // Encrypted note
        let note = MoneyNote {
            serial,
            value: output.value,
            token_id: output.token_id,
            spend_hook,
            user_data: user_data_enc,
            coin_blind,
            value_blind: self.value_blind,
            token_blind: self.token_blind,
            memo: vec![],
        };

        let encrypted_note = AeadEncryptedNote::encrypt(&note, &output.public_key, &mut OsRng)?;

        let output = Output {
            value_commit: public_inputs.value_commit,
            token_commit: public_inputs.token_commit,
            coin: public_inputs.coin,
            note: encrypted_note,
        };

        let input = StakeInput {
            token_blind: self.token_blind,
            value_commit: public_inputs.value_commit,
            nullifier: self.nullifier,
            merkle_root: self.merkle_root,
            signature_public: self.signature_public,
        };

        // We now fill this with necessary stuff
        let params = MoneyUnstakeParamsV1 { input, spend_hook, user_data_enc, output };
        let proofs = vec![proof];

        // Now we should have all the params and zk proof.
        // We return it all and let the caller deal with it.
        let debris = MoneyUnstakeCallDebris { params, proofs };
        Ok(debris)
    }
}

pub fn create_unstake_mint_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    output: &TransactionBuilderOutputInfo,
    value_blind: pallas::Scalar,
    token_blind: pallas::Scalar,
    serial: pallas::Base,
    spend_hook: pallas::Base,
    user_data: pallas::Base,
    coin_blind: pallas::Base,
) -> Result<(Proof, MoneyMintRevealed)> {
    let value_commit = pedersen_commitment_u64(output.value, value_blind);
    let token_commit = pedersen_commitment_base(output.token_id.inner(), token_blind);
    let (pub_x, pub_y) = output.public_key.xy();

    let coin = Coin::from(poseidon_hash([
        pub_x,
        pub_y,
        pallas::Base::from(output.value),
        output.token_id.inner(),
        serial,
        spend_hook,
        user_data,
        coin_blind,
    ]));

    let public_inputs = MoneyMintRevealed { coin, value_commit, token_commit };

    let prover_witnesses = vec![
        Witness::Base(Value::known(pub_x)),
        Witness::Base(Value::known(pub_y)),
        Witness::Base(Value::known(pallas::Base::from(output.value))),
        Witness::Base(Value::known(output.token_id.inner())),
        Witness::Base(Value::known(serial)),
        Witness::Base(Value::known(coin_blind)),
        Witness::Base(Value::known(spend_hook)),
        Witness::Base(Value::known(user_data)),
        Witness::Scalar(Value::known(value_blind)),
        Witness::Scalar(Value::known(token_blind)),
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin.clone());
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
