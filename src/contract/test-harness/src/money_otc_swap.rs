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

use darkfi::{
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    zk::halo2::Field,
    Result,
};
use darkfi_money_contract::{
    client::{swap_v1::SwapCallBuilder, MoneyNote, OwnCoin},
    model::{MoneyFeeParamsV1, MoneyTransferParamsV1},
    MoneyFunction, MONEY_CONTRACT_ZKAS_BURN_NS_V1, MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};
use darkfi_sdk::{
    crypto::{contract_id::MONEY_CONTRACT_ID, BaseBlind, Blind, FuncId, MerkleNode},
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::AsyncEncodable;
use log::debug;
use rand::rngs::OsRng;

use super::{Holder, TestHarness};

impl TestHarness {
    /// Create a `Money::OtcSwap` transaction with two given [`Holder`]s.
    ///
    /// Returns the [`Transaction`], and the transaction parameters.
    pub async fn otc_swap(
        &mut self,
        holder0: &Holder,
        owncoin0: &OwnCoin,
        holder1: &Holder,
        owncoin1: &OwnCoin,
        block_height: u64,
    ) -> Result<(Transaction, MoneyTransferParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet0 = self.holders.get(holder0).unwrap();
        let wallet1 = self.holders.get(holder1).unwrap();

        let (mint_pk, mint_zkbin) =
            self.proving_keys.get(&MONEY_CONTRACT_ZKAS_MINT_NS_V1.to_string()).unwrap();

        let (burn_pk, burn_zkbin) =
            self.proving_keys.get(&MONEY_CONTRACT_ZKAS_BURN_NS_V1.to_string()).unwrap();

        // Use a zero spend_hook and user_data
        let rcpt_spend_hook = FuncId::none();
        let rcpt_user_data = pallas::Base::ZERO;
        let rcpt_user_data_blind = Blind::random(&mut OsRng);

        // Create blinding factors for commitments
        let value_send_blind = Blind::random(&mut OsRng);
        let value_recv_blind = Blind::random(&mut OsRng);
        let token_send_blind = BaseBlind::random(&mut OsRng);
        let token_recv_blind = BaseBlind::random(&mut OsRng);

        // Build the first half of the swap for Holder0
        let builder = SwapCallBuilder {
            pubkey: wallet0.keypair.public,
            value_send: owncoin0.note.value,
            token_id_send: owncoin0.note.token_id,
            value_recv: owncoin1.note.value,
            token_id_recv: owncoin1.note.token_id,
            user_data_blind_send: rcpt_user_data_blind,
            spend_hook_recv: rcpt_spend_hook,
            user_data_recv: rcpt_user_data,
            value_blinds: [value_send_blind, value_recv_blind],
            token_blinds: [token_send_blind, token_recv_blind],
            coin: owncoin0.clone(),
            tree: wallet0.money_merkle_tree.clone(),
            mint_zkbin: mint_zkbin.clone(),
            mint_pk: mint_pk.clone(),
            burn_zkbin: burn_zkbin.clone(),
            burn_pk: burn_pk.clone(),
        };

        let debris0 = builder.build()?;
        assert!(debris0.params.inputs.len() == 1);
        assert!(debris0.params.outputs.len() == 1);

        // Build the second half of the swap for Holder1
        let builder = SwapCallBuilder {
            pubkey: wallet1.keypair.public,
            value_send: owncoin1.note.value,
            token_id_send: owncoin1.note.token_id,
            value_recv: owncoin0.note.value,
            token_id_recv: owncoin0.note.token_id,
            user_data_blind_send: rcpt_user_data_blind,
            spend_hook_recv: rcpt_spend_hook,
            user_data_recv: rcpt_user_data,
            value_blinds: [value_recv_blind, value_send_blind],
            token_blinds: [token_recv_blind, token_send_blind],
            coin: owncoin1.clone(),
            tree: wallet1.money_merkle_tree.clone(),
            mint_zkbin: mint_zkbin.clone(),
            mint_pk: mint_pk.clone(),
            burn_zkbin: burn_zkbin.clone(),
            burn_pk: burn_pk.clone(),
        };

        let debris1 = builder.build()?;
        assert!(debris1.params.inputs.len() == 1);
        assert!(debris1.params.outputs.len() == 1);

        // Holder1 then combines the halves
        let swap_full_params = MoneyTransferParamsV1 {
            inputs: vec![debris0.params.inputs[0].clone(), debris1.params.inputs[0].clone()],
            outputs: vec![debris0.params.outputs[0].clone(), debris1.params.outputs[0].clone()],
        };

        let swap_full_proofs = vec![
            debris0.proofs[0].clone(),
            debris1.proofs[0].clone(),
            debris0.proofs[1].clone(),
            debris1.proofs[1].clone(),
        ];

        // Encode the contract call
        let mut data = vec![MoneyFunction::OtcSwapV1 as u8];
        swap_full_params.encode_async(&mut data).await?;
        let call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };
        let mut tx_builder =
            TransactionBuilder::new(ContractCallLeaf { call, proofs: swap_full_proofs }, vec![])?;

        // If we have tx fees enabled, make an offering
        let mut fee_params = None;
        let mut fee_signature_secrets = None;
        if self.verify_fees {
            let mut tx = tx_builder.build()?;
            let sigs = tx.create_sigs(&[debris1.signature_secret])?;
            tx.signatures = vec![sigs];

            // First holder gets the partially signed transaction and adds their signature
            let sigs = tx.create_sigs(&[debris0.signature_secret])?;
            tx.signatures[0].insert(0, sigs[0]);

            let (fee_call, fee_proofs, fee_secrets, _spent_fee_coins, fee_call_params) =
                self.append_fee_call(holder0, tx, block_height, &[owncoin0.clone()]).await?;

            // Append the fee call to the transaction
            tx_builder.append(ContractCallLeaf { call: fee_call, proofs: fee_proofs }, vec![])?;
            fee_signature_secrets = Some(fee_secrets);
            fee_params = Some(fee_call_params);
        }

        // Now build the actual transaction and sign it with necessary keys.
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&[debris1.signature_secret])?;
        tx.signatures = vec![sigs];
        // First holder gets the partially signed transaction and adds their signature
        let sigs = tx.create_sigs(&[debris0.signature_secret])?;
        tx.signatures[0].insert(0, sigs[0]);

        if let Some(fee_signature_secrets) = fee_signature_secrets {
            let sigs = tx.create_sigs(&fee_signature_secrets)?;
            tx.signatures.push(sigs);
        }

        Ok((tx, swap_full_params, fee_params))
    }

    /// Execute the transaction created by `otc_swap()` for a given [`Holder`].
    ///
    /// Returns any found [`OwnCoin`]s.
    pub async fn execute_otc_swap_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        swap_params: &MoneyTransferParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u64,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.holders.get_mut(holder).unwrap();

        // Execute the transaction
        wallet.validator.add_transactions(&[tx], block_height, true, self.verify_fees).await?;

        let mut found_owncoins = vec![];

        if !append {
            return Ok(found_owncoins)
        }

        let mut inputs = swap_params.inputs.to_vec();
        let mut outputs = swap_params.outputs.to_vec();
        if let Some(ref fee_params) = fee_params {
            inputs.push(fee_params.input.clone());
            outputs.push(fee_params.output.clone());
        }

        for input in inputs {
            if let Some(spent_coin) = wallet
                .unspent_money_coins
                .iter()
                .find(|x| x.nullifier() == input.nullifier)
                .cloned()
            {
                debug!("Found spent OwnCoin({}) for {:?}", spent_coin.coin, holder);
                wallet.unspent_money_coins.retain(|x| x.nullifier() != input.nullifier);
                wallet.spent_money_coins.push(spent_coin.clone());
            }
        }

        for output in outputs {
            wallet.money_merkle_tree.append(MerkleNode::from(output.coin.inner()));

            // Attempt to decrypt the encrypted note
            let Ok(note) = output.note.decrypt::<MoneyNote>(&wallet.keypair.secret) else {
                continue
            };

            let owncoin = OwnCoin {
                coin: output.coin,
                note: note.clone(),
                secret: wallet.keypair.secret,
                leaf_position: wallet.money_merkle_tree.mark().unwrap(),
            };

            debug!("Found new OwnCoin({}) for {:?}", owncoin.coin, holder);
            wallet.unspent_money_coins.push(owncoin.clone());
            found_owncoins.push(owncoin);
        }

        Ok(found_owncoins)
    }
}
