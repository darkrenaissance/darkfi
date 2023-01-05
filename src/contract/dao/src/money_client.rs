/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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
    zk::proof::{Proof, ProvingKey},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{pedersen::ValueBlind, MerkleNode, PublicKey, SecretKey, TokenId},
    incrementalmerkletree,
    pasta::{group::ff::Field, pallas},
};

use rand::rngs::OsRng;

use darkfi_money_contract::{
    client::{create_transfer_burn_proof, create_transfer_mint_proof, Note},
    state::{ClearInput, Input, MoneyTransferParams, Output},
};

/*
use darkfi::{
    crypto::{
        burn_proof::create_burn_proof,
        mint_proof::create_mint_proof,
        types::{
            DrkCoinBlind, DrkSerial, DrkSpendHook, DrkUserData, DrkUserDataBlind, DrkValueBlind,
        },
    },
    Result,
};

use crate::{
    contract::money::{
        transfer::validate::{CallData, ClearInput, Input, Output},
        CONTRACT_ID,
    },
    note,
    util::{FuncCall, ZkContractInfo, ZkContractTable},
};
*/

/*
#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct Note {
    pub serial: DrkSerial,
    pub value: u64,
    pub token_id: TokenId,
    pub spend_hook: DrkSpendHook,
    pub user_data: DrkUserData,
    pub coin_blind: DrkCoinBlind,
    pub value_blind: DrkValueBlind,
    pub token_blind: DrkValueBlind,
}
*/

pub struct Builder {
    pub clear_inputs: Vec<BuilderClearInputInfo>,
    pub inputs: Vec<BuilderInputInfo>,
    pub outputs: Vec<BuilderOutputInfo>,
}

pub struct BuilderClearInputInfo {
    pub value: u64,
    pub token_id: TokenId,
    pub signature_secret: SecretKey,
}

pub struct BuilderInputInfo {
    pub leaf_position: incrementalmerkletree::Position,
    pub merkle_path: Vec<MerkleNode>,
    pub secret: SecretKey,
    pub note: Note,
    pub user_data_blind: pallas::Base,
    pub value_blind: ValueBlind,
    pub signature_secret: SecretKey,
}

pub struct BuilderOutputInfo {
    pub value: u64,
    pub token_id: TokenId,
    pub public: PublicKey,
    pub serial: pallas::Base,
    pub coin_blind: pallas::Base,
    pub spend_hook: pallas::Base,
    pub user_data: pallas::Base,
}

impl Builder {
    fn compute_remainder_blind(
        clear_inputs: &[ClearInput],
        input_blinds: &[ValueBlind],
        output_blinds: &[ValueBlind],
    ) -> ValueBlind {
        let mut total = ValueBlind::zero();

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

    pub fn build(
        self,
        mint_zkbin: &ZkBinary,
        mint_pk: &ProvingKey,
        burn_zkbin: &ZkBinary,
        burn_pk: &ProvingKey,
    ) -> Result<(MoneyTransferParams, Vec<Proof>)> {
        assert!(self.clear_inputs.len() + self.inputs.len() > 0);

        let mut clear_inputs = vec![];
        let token_blind = ValueBlind::random(&mut OsRng);
        for input in &self.clear_inputs {
            let signature_public = PublicKey::from_secret(input.signature_secret);
            let value_blind = ValueBlind::random(&mut OsRng);

            let clear_input = ClearInput {
                value: input.value,
                token_id: input.token_id,
                value_blind,
                token_blind,
                signature_public,
            };
            clear_inputs.push(clear_input);
        }

        let mut proofs = vec![];
        let mut inputs = vec![];
        let mut input_blinds = vec![];

        for input in self.inputs {
            let value_blind = input.value_blind;
            input_blinds.push(value_blind);

            // Note from the previous output
            let note = input.note.clone();

            let (proof, revealed) = create_transfer_burn_proof(
                burn_zkbin,
                burn_pk,
                note.value,
                note.token_id,
                value_blind,
                token_blind,
                note.serial,
                note.spend_hook,
                note.user_data,
                input.user_data_blind,
                note.coin_blind,
                input.secret,
                input.leaf_position,
                input.merkle_path.clone(),
                input.signature_secret,
            )?;

            proofs.push(proof);

            let input = Input {
                value_commit: revealed.value_commit,
                token_commit: revealed.token_commit,
                nullifier: revealed.nullifier,
                merkle_root: revealed.merkle_root,
                spend_hook: revealed.spend_hook,
                user_data_enc: revealed.user_data_enc,
                signature_public: revealed.signature_public,
            };
            inputs.push(input);
        }

        let mut outputs = vec![];
        let mut output_blinds = vec![];
        // This value_blind calc assumes there will always be at least a single output
        assert!(!self.outputs.is_empty());

        for (i, output) in self.outputs.iter().enumerate() {
            let value_blind = if i == self.outputs.len() - 1 {
                Self::compute_remainder_blind(&clear_inputs, &input_blinds, &output_blinds)
            } else {
                ValueBlind::random(&mut OsRng)
            };
            output_blinds.push(value_blind);

            let serial = output.serial;
            let coin_blind = output.coin_blind;

            let (proof, revealed) = create_transfer_mint_proof(
                mint_zkbin,
                mint_pk,
                output.value,
                output.token_id,
                value_blind,
                token_blind,
                serial,
                output.spend_hook,
                output.user_data,
                coin_blind,
                output.public,
            )?;

            proofs.push(proof);

            let note = Note {
                serial,
                value: output.value,
                token_id: output.token_id,
                spend_hook: output.spend_hook,
                user_data: output.user_data,
                coin_blind,
                value_blind,
                token_blind,
                memo: Vec::new(),
            };

            let encrypted_note = note.encrypt(&output.public)?;

            let output = Output {
                value_commit: revealed.value_commit,
                token_commit: revealed.token_commit,
                coin: revealed.coin.inner(),
                ciphertext: encrypted_note.ciphertext,
                ephem_public: encrypted_note.ephem_public,
            };
            outputs.push(output);
        }

        Ok((MoneyTransferParams { clear_inputs, inputs, outputs }, proofs))
    }
}
