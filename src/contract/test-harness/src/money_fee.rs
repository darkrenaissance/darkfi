/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use std::{collections::HashSet, hash::RandomState};

use darkfi::{
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    zk::{halo2::Field, Proof},
    Result,
};
use darkfi_money_contract::{
    client::{
        compute_remainder_blind,
        fee_v1::{create_fee_proof, FeeCallInput, FeeCallOutput, FEE_CALL_GAS},
        MoneyNote, OwnCoin,
    },
    model::{token_id::DARK_TOKEN_ID, Input, MoneyFeeParamsV1, Output},
    MoneyFunction, MONEY_CONTRACT_ZKAS_FEE_NS_V1,
};
use darkfi_sdk::{
    crypto::{
        contract_id::MONEY_CONTRACT_ID, note::AeadEncryptedNote, BaseBlind, Blind, FuncId,
        MerkleNode, ScalarBlind, SecretKey,
    },
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::AsyncEncodable;
use log::{debug, info};
use rand::rngs::OsRng;

use super::{Holder, TestHarness};

impl TestHarness {
    /// Create an empty transaction that includes a `Money::Fee` call.
    /// This is generally used to test the actual fee call, and also to
    /// see the gas usage of the call without other parts.
    pub async fn create_empty_fee_call(
        &mut self,
        holder: &Holder,
    ) -> Result<(Transaction, MoneyFeeParamsV1)> {
        let wallet = self.holders.get(holder).unwrap();

        // Find a compatible OwnCoin
        let coin = wallet
            .unspent_money_coins
            .iter()
            .find(|x| x.note.token_id == *DARK_TOKEN_ID && x.note.value > FEE_CALL_GAS)
            .unwrap();

        // Input and output setup
        let input = FeeCallInput {
            leaf_position: coin.leaf_position,
            merkle_path: wallet.money_merkle_tree.witness(coin.leaf_position, 0).unwrap(),
            secret: coin.secret,
            note: coin.note.clone(),
            user_data_blind: Blind::random(&mut OsRng),
        };

        let output = FeeCallOutput {
            public_key: wallet.keypair.public,
            value: coin.note.value - FEE_CALL_GAS,
            token_id: coin.note.token_id,
            blind: Blind::random(&mut OsRng),
            spend_hook: FuncId::none(),
            user_data: pallas::Base::ZERO,
        };

        // Generate blinding factors
        let token_blind = BaseBlind::random(&mut OsRng);
        let input_value_blind = ScalarBlind::random(&mut OsRng);
        let fee_value_blind = ScalarBlind::random(&mut OsRng);
        let output_value_blind = compute_remainder_blind(&[input_value_blind], &[fee_value_blind]);

        // Generate an ephemeral signing key
        let signature_secret = SecretKey::random(&mut OsRng);

        info!("Creting FeeV1 ZK proof");
        let (fee_pk, fee_zkbin) =
            self.proving_keys.get(&MONEY_CONTRACT_ZKAS_FEE_NS_V1.to_string()).unwrap();

        let (proof, public_inputs) = create_fee_proof(
            fee_zkbin,
            fee_pk,
            &input,
            input_value_blind,
            &output,
            output_value_blind,
            output.spend_hook,
            output.user_data,
            output.blind,
            token_blind,
            signature_secret,
        )?;

        // Encrypted note for the output
        let note = MoneyNote {
            coin_blind: output.blind,
            value: output.value,
            token_id: output.token_id,
            spend_hook: output.spend_hook,
            user_data: output.user_data,
            value_blind: output_value_blind,
            token_blind,
            memo: vec![],
        };

        let encrypted_note = AeadEncryptedNote::encrypt(&note, &output.public_key, &mut OsRng)?;

        let params = MoneyFeeParamsV1 {
            input: Input {
                value_commit: public_inputs.input_value_commit,
                token_commit: public_inputs.token_commit,
                nullifier: public_inputs.nullifier,
                merkle_root: public_inputs.merkle_root,
                user_data_enc: public_inputs.input_user_data_enc,
                signature_public: public_inputs.signature_public,
            },
            output: Output {
                value_commit: public_inputs.output_value_commit,
                token_commit: public_inputs.token_commit,
                coin: public_inputs.output_coin,
                note: encrypted_note,
            },
            fee_value_blind,
            token_blind,
        };

        let mut data = vec![MoneyFunction::FeeV1 as u8];
        FEE_CALL_GAS.encode_async(&mut data).await?;
        params.encode_async(&mut data).await?;
        let call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };
        let mut tx_builder =
            TransactionBuilder::new(ContractCallLeaf { call, proofs: vec![proof] }, vec![])?;
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&[signature_secret])?;
        tx.signatures = vec![sigs];

        Ok((tx, params))
    }

    /// Execute the transaction created by `create_empty_fee_call()` for a given [`Holder`]
    ///
    /// Returns any found [`OwnCoin`]s.
    pub async fn execute_empty_fee_call_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        params: &MoneyFeeParamsV1,
        block_height: u64,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.holders.get_mut(holder).unwrap();

        wallet.validator.add_transactions(&[tx], block_height, true, self.verify_fees).await?;
        wallet.money_merkle_tree.append(MerkleNode::from(params.output.coin.inner()));

        // Attempt to decrypt the output note to see if this is a coin for the holder
        let Ok(note) = params.output.note.decrypt::<MoneyNote>(&wallet.keypair.secret) else {
            return Ok(vec![])
        };

        let owncoin = OwnCoin {
            coin: params.output.coin,
            note: note.clone(),
            secret: wallet.keypair.secret,
            leaf_position: wallet.money_merkle_tree.mark().unwrap(),
        };

        let spent_coin = wallet
            .unspent_money_coins
            .iter()
            .find(|x| x.nullifier() == params.input.nullifier)
            .unwrap()
            .clone();

        debug!("Found spent OwnCoin({}) for {:?}", spent_coin.coin, holder);
        debug!("Found new OwnCoin({}) for {:?}", owncoin.coin, holder);

        wallet.unspent_money_coins.retain(|x| x.nullifier() != params.input.nullifier);
        wallet.spent_money_coins.push(spent_coin);
        wallet.unspent_money_coins.push(owncoin.clone());

        Ok(vec![owncoin])
    }

    /// Create and append a `Money::Fee` call to a given [`Transaction`] for
    /// a given [`Holder`].
    ///
    /// Additionally takes a set of spent coins in order not to reuse them here.
    ///
    /// Returns the `Fee` call, and all necessary data and parameters related.
    pub async fn append_fee_call(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        block_height: u64,
        spent_coins: &[OwnCoin],
    ) -> Result<(ContractCall, Vec<Proof>, Vec<SecretKey>, Vec<OwnCoin>, MoneyFeeParamsV1)> {
        // First we verify the fee-less transaction to see how much gas it uses for execution
        // and verification.
        let wallet = self.holders.get(holder).unwrap();
        let mut gas_used = FEE_CALL_GAS;
        gas_used += wallet.validator.add_transactions(&[tx], block_height, false, false).await?;

        // Knowing the total gas, we can now find an OwnCoin of enough value
        // so that we can create a valid Money::Fee call.
        let spent_coins: HashSet<&OwnCoin, RandomState> = HashSet::from_iter(spent_coins);
        let mut available_coins = wallet.unspent_money_coins.clone();
        available_coins.retain(|x| x.note.token_id == *DARK_TOKEN_ID && x.note.value > gas_used);
        available_coins.retain(|x| !spent_coins.contains(x));
        assert!(!available_coins.is_empty());

        let coin = &available_coins[0];
        let change_value = coin.note.value - gas_used;

        // Input and output setup
        let input = FeeCallInput {
            leaf_position: coin.leaf_position,
            merkle_path: wallet.money_merkle_tree.witness(coin.leaf_position, 0).unwrap(),
            secret: coin.secret,
            note: coin.note.clone(),
            user_data_blind: BaseBlind::random(&mut OsRng),
        };

        let output = FeeCallOutput {
            public_key: wallet.keypair.public,
            value: change_value,
            token_id: coin.note.token_id,
            blind: BaseBlind::random(&mut OsRng),
            spend_hook: FuncId::none(),
            user_data: pallas::Base::ZERO,
        };

        // Create blinding factors
        let token_blind = BaseBlind::random(&mut OsRng);
        let input_value_blind = ScalarBlind::random(&mut OsRng);
        let fee_value_blind = ScalarBlind::random(&mut OsRng);
        let output_value_blind = compute_remainder_blind(&[input_value_blind], &[fee_value_blind]);

        // Create an ephemeral signing key
        let signature_secret = SecretKey::random(&mut OsRng);

        info!("Creating FeeV1 ZK proof");
        let (fee_pk, fee_zkbin) =
            self.proving_keys.get(&MONEY_CONTRACT_ZKAS_FEE_NS_V1.to_string()).unwrap();

        let (proof, public_inputs) = create_fee_proof(
            fee_zkbin,
            fee_pk,
            &input,
            input_value_blind,
            &output,
            output_value_blind,
            output.spend_hook,
            output.user_data,
            output.blind,
            token_blind,
            signature_secret,
        )?;

        // Encrypted note for the output
        let note = MoneyNote {
            coin_blind: output.blind,
            value: output.value,
            token_id: output.token_id,
            spend_hook: output.spend_hook,
            user_data: output.user_data,
            value_blind: output_value_blind,
            token_blind,
            memo: vec![],
        };

        let encrypted_note = AeadEncryptedNote::encrypt(&note, &output.public_key, &mut OsRng)?;

        let params = MoneyFeeParamsV1 {
            input: Input {
                value_commit: public_inputs.input_value_commit,
                token_commit: public_inputs.token_commit,
                nullifier: public_inputs.nullifier,
                merkle_root: public_inputs.merkle_root,
                user_data_enc: public_inputs.input_user_data_enc,
                signature_public: public_inputs.signature_public,
            },
            output: Output {
                value_commit: public_inputs.output_value_commit,
                token_commit: public_inputs.token_commit,
                coin: public_inputs.output_coin,
                note: encrypted_note,
            },
            fee_value_blind,
            token_blind,
        };

        // Encode the contract call
        let mut data = vec![MoneyFunction::FeeV1 as u8];
        gas_used.encode_async(&mut data).await?;
        params.encode_async(&mut data).await?;
        let call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };

        Ok((call, vec![proof], vec![signature_secret], vec![coin.clone()], params))
    }
}
