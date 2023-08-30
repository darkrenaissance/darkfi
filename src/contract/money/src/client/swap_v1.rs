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
    zk::{Proof, ProvingKey},
    zkas::ZkBinary,
    ClientFailed, Result,
};
use darkfi_sdk::{
    crypto::{
        note::AeadEncryptedNote, pasta_prelude::*, MerkleTree, PublicKey, SecretKey, TokenId,
    },
    pasta::pallas,
};
use darkfi_serial::serialize;
use log::{debug, info};
use rand::rngs::OsRng;

use crate::{
    client::{
        transfer_v1::{
            create_transfer_burn_proof, create_transfer_mint_proof, TransactionBuilderInputInfo,
            TransactionBuilderOutputInfo,
        },
        MoneyNote, OwnCoin,
    },
    model::{Input, MoneyTransferParamsV1, Output},
};

pub struct SwapCallDebris {
    pub params: MoneyTransferParamsV1,
    pub proofs: Vec<Proof>,
    pub signature_secret: SecretKey,
}

/// Struct holding necessary information to build a `Money::OtcSwapV1` contract call.
/// This is used to build half of the swap transaction, so both parties have to build
/// their halves and combine them.
pub struct SwapCallBuilder {
    /// Party's public key for receiving the output
    pub pubkey: PublicKey,
    /// The value of the party's input to swap (send)
    pub value_send: u64,
    /// The token ID of the party's input to swap (send)
    pub token_id_send: TokenId,
    /// The value of the party's output to receive
    pub value_recv: u64,
    /// The token ID of the party's output to receive
    pub token_id_recv: TokenId,
    /// User data blind for the party's input
    pub user_data_blind_send: pallas::Base,
    /// Spend hook for the party's output
    pub spend_hook_recv: pallas::Base,
    /// User data for the party's output
    pub user_data_recv: pallas::Base,
    /// The blinds to be used for value pedersen commitments
    /// `[0]` is used for input 0 and output 1, and `[1]` is
    /// used for input 1 and output 0. The same applies to
    /// `token_blinds`.
    pub value_blinds: [pallas::Scalar; 2],
    /// The blinds to be used for token ID pedersen commitments
    pub token_blinds: [pallas::Base; 2],
    /// The coin to be used as the input to the swap
    pub coin: OwnCoin,
    /// Merkle tree of coins used to create inclusion proofs
    pub tree: MerkleTree,
    /// `Mint_V1` zkas circuit ZkBinary
    pub mint_zkbin: ZkBinary,
    /// Proving key for the `Mint_V1` zk circuit
    pub mint_pk: ProvingKey,
    /// `Burn_V1` zkas circuit ZkBinary
    pub burn_zkbin: ZkBinary,
    /// Proving key for the `Burn_V1` zk circuit
    pub burn_pk: ProvingKey,
}

impl SwapCallBuilder {
    pub fn build(&self) -> Result<SwapCallDebris> {
        debug!("Building half of Money::OtcSwapV1 contract call");
        assert!(self.value_send != 0);
        assert!(self.value_recv != 0);
        assert!(self.token_id_send.inner() != pallas::Base::zero());
        assert!(self.token_id_recv.inner() != pallas::Base::zero());

        if self.coin.note.value != self.value_send {
            return Err(ClientFailed::InvalidAmount(self.coin.note.value).into())
        }

        if self.coin.note.token_id != self.token_id_send {
            return Err(ClientFailed::InvalidTokenId(self.coin.note.token_id.to_string()).into())
        }

        let leaf_position = self.coin.leaf_position;
        let merkle_path = self.tree.witness(leaf_position, 0).unwrap();

        let input = TransactionBuilderInputInfo {
            leaf_position,
            merkle_path,
            secret: self.coin.secret,
            note: self.coin.note.clone(),
        };

        let output = TransactionBuilderOutputInfo {
            value: self.value_recv,
            token_id: self.token_id_recv,
            public_key: self.pubkey,
        };

        // Now we fill this with necessary stuff
        let mut params =
            MoneyTransferParamsV1 { clear_inputs: vec![], inputs: vec![], outputs: vec![] };

        // Create a new ephemeral secret key
        let signature_secret = SecretKey::random(&mut OsRng);

        let mut proofs = vec![];
        info!("Creating burn proof for input");
        let (proof, public_inputs) = create_transfer_burn_proof(
            &self.burn_zkbin,
            &self.burn_pk,
            &input,
            self.value_blinds[0],
            self.token_blinds[0],
            self.user_data_blind_send,
            signature_secret,
        )?;

        params.inputs.push(Input {
            value_commit: public_inputs.value_commit,
            token_commit: public_inputs.token_commit,
            nullifier: public_inputs.nullifier,
            merkle_root: public_inputs.merkle_root,
            spend_hook: public_inputs.spend_hook,
            user_data_enc: public_inputs.user_data_enc,
            signature_public: public_inputs.signature_public,
        });

        proofs.push(proof);

        // For the output, we create a new serial
        let serial = pallas::Base::random(&mut OsRng);

        info!("Creating mint proof for output");
        let (proof, public_inputs) = create_transfer_mint_proof(
            &self.mint_zkbin,
            &self.mint_pk,
            &output,
            self.value_blinds[1],
            self.token_blinds[1],
            serial,
            self.spend_hook_recv,
            self.user_data_recv,
        )?;

        proofs.push(proof);

        // Encrypted note
        let note = MoneyNote {
            serial,
            value: output.value,
            token_id: output.token_id,
            spend_hook: self.spend_hook_recv,
            user_data: self.user_data_recv,
            value_blind: self.value_blinds[1],
            token_blind: self.token_blinds[1],
            // Here we store our secret key we use for signing
            memo: serialize(&signature_secret),
        };

        let encrypted_note = AeadEncryptedNote::encrypt(&note, &self.pubkey, &mut OsRng)?;

        params.outputs.push(Output {
            value_commit: public_inputs.value_commit,
            token_commit: public_inputs.token_commit,
            coin: public_inputs.coin,
            note: encrypted_note,
        });

        // Now we should have all the params, zk proofs, and signature secrets.
        // We return it all and let the caller deal with it.
        let debris = SwapCallDebris { params, proofs, signature_secret };
        Ok(debris)
    }
}
