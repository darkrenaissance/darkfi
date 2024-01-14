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

use darkfi::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{
        note::AeadEncryptedNote, pasta_prelude::*, pedersen_commitment_u64, poseidon_hash, Keypair,
        PublicKey, TokenId,
    },
    pasta::pallas,
};
use log::info;
use rand::rngs::OsRng;

use crate::{
    client::{
        transfer_v1::{TransferCallClearInput, TransferCallOutput},
        MoneyNote,
    },
    model::{ClearInput, Coin, MoneyTokenMintParamsV1, Output},
};

pub struct TokenMintCallDebris {
    pub params: MoneyTokenMintParamsV1,
    pub proofs: Vec<Proof>,
}

pub struct TokenMintRevealed {
    pub signature_public: PublicKey,
    pub token_id: TokenId,
    pub coin: Coin,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
}

impl TokenMintRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let (sig_x, sig_y) = self.signature_public.xy();
        let valcom_coords = self.value_commit.to_affine().coordinates().unwrap();

        // NOTE: It's important to keep these in the same order
        // as the `constrain_instance` calls in the zkas code.
        vec![
            sig_x,
            sig_y,
            self.token_id.inner(),
            self.coin.inner(),
            *valcom_coords.x(),
            *valcom_coords.y(),
            self.token_commit,
        ]
    }
}

/// Struct holding necessary information to build a `Money::TokenMintV1` contract call.
pub struct TokenMintCallBuilder {
    /// Mint authority keypair
    pub mint_authority: Keypair,
    /// Recipient of the minted tokens
    pub recipient: PublicKey,
    /// Amount of tokens we want to mint
    pub amount: u64,
    /// Spend hook for the output
    pub spend_hook: pallas::Base,
    /// User data for the output
    pub user_data: pallas::Base,
    /// `TokenMint_V1` zkas circuit ZkBinary
    pub token_mint_zkbin: ZkBinary,
    /// Proving key for the `TokenMint_V1` zk circuit,
    pub token_mint_pk: ProvingKey,
}

impl TokenMintCallBuilder {
    pub fn build(&self) -> Result<TokenMintCallDebris> {
        info!("Building Money::TokenMintV1 contract call");
        assert!(self.amount != 0);

        // In this call, we will build one clear input and one anonymous output.
        // The mint authority pubkey is used to derive the token ID.
        let token_id = TokenId::derive(self.mint_authority.secret);

        let input = TransferCallClearInput {
            value: self.amount,
            token_id,
            signature_secret: self.mint_authority.secret,
        };

        let output = TransferCallOutput {
            public_key: self.recipient,
            value: self.amount,
            token_id,
            serial: pallas::Base::random(&mut OsRng),
            spend_hook: pallas::Base::ZERO,
            user_data: pallas::Base::ZERO,
        };

        // We just create the pedersen commitment blinds here. We simply
        // enforce that the clear input and the anon output have the same
        // commitments. Not sure if this can be avoided, but also is it
        // really necessary to avoid?
        let value_blind = pallas::Scalar::random(&mut OsRng);
        let token_blind = pallas::Base::random(&mut OsRng);

        let c_input = ClearInput {
            value: input.value,
            token_id: input.token_id,
            value_blind,
            token_blind,
            signature_public: PublicKey::from_secret(input.signature_secret),
        };

        let serial = pallas::Base::random(&mut OsRng);

        info!("Creating token mint proof for output");
        let (proof, public_inputs) = create_token_mint_proof(
            &self.token_mint_zkbin,
            &self.token_mint_pk,
            &output,
            &self.mint_authority,
            value_blind,
            token_blind,
            serial,
            self.spend_hook,
            self.user_data,
        )?;

        let note = MoneyNote {
            serial,
            value: output.value,
            token_id: output.token_id,
            spend_hook: self.spend_hook,
            user_data: self.user_data,
            value_blind,
            token_blind,
            memo: vec![],
        };

        let encrypted_note = AeadEncryptedNote::encrypt(&note, &output.public_key, &mut OsRng)?;

        let c_output = Output {
            value_commit: public_inputs.value_commit,
            token_commit: public_inputs.token_commit,
            coin: public_inputs.coin,
            note: encrypted_note,
        };

        let params = MoneyTokenMintParamsV1 { input: c_input, output: c_output };
        let debris = TokenMintCallDebris { params, proofs: vec![proof] };
        Ok(debris)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn create_token_mint_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    output: &TransferCallOutput,
    mint_authority: &Keypair,
    value_blind: pallas::Scalar,
    token_blind: pallas::Base,
    serial: pallas::Base,
    spend_hook: pallas::Base,
    user_data: pallas::Base,
) -> Result<(Proof, TokenMintRevealed)> {
    let token_id = TokenId::derive(mint_authority.secret);

    let value_commit = pedersen_commitment_u64(output.value, value_blind);
    let token_commit = poseidon_hash([token_id.inner(), token_blind]);

    let (rcpt_x, rcpt_y) = output.public_key.xy();

    let coin = Coin::from(poseidon_hash([
        rcpt_x,
        rcpt_y,
        pallas::Base::from(output.value),
        token_id.inner(),
        serial,
        spend_hook,
        user_data,
    ]));

    let public_inputs = TokenMintRevealed {
        signature_public: mint_authority.public,
        token_id,
        coin,
        value_commit,
        token_commit,
    };

    let prover_witnesses = vec![
        Witness::Base(Value::known(mint_authority.secret.inner())),
        Witness::Base(Value::known(pallas::Base::from(output.value))),
        Witness::Base(Value::known(rcpt_x)),
        Witness::Base(Value::known(rcpt_y)),
        Witness::Base(Value::known(serial)),
        Witness::Base(Value::known(spend_hook)),
        Witness::Base(Value::known(user_data)),
        Witness::Scalar(Value::known(value_blind)),
        Witness::Base(Value::known(token_blind)),
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
