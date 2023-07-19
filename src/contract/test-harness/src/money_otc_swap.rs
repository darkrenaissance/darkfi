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

use std::time::Instant;

use darkfi::{tx::Transaction, zk::halo2::Field, Result};
use darkfi_money_contract::{
    client::{swap_v1::SwapCallBuilder, OwnCoin},
    model::MoneyTransferParamsV1,
    MoneyFunction, MONEY_CONTRACT_ZKAS_BURN_NS_V1, MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};
use darkfi_sdk::{
    crypto::{MerkleNode, ValueBlind, MONEY_CONTRACT_ID},
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{serialize, Encodable};
use rand::rngs::OsRng;

use super::{Holder, TestHarness, TxAction};

impl TestHarness {
    pub fn otc_swap(
        &mut self,
        holder0: &Holder,
        owncoin0: &OwnCoin,
        holder1: &Holder,
        owncoin1: &OwnCoin,
    ) -> Result<(Transaction, MoneyTransferParamsV1)> {
        let wallet0 = self.holders.get(holder0).unwrap();
        let wallet1 = self.holders.get(holder1).unwrap();
        let (mint_pk, mint_zkbin) = self.proving_keys.get(&MONEY_CONTRACT_ZKAS_MINT_NS_V1).unwrap();
        let (burn_pk, burn_zkbin) = self.proving_keys.get(&MONEY_CONTRACT_ZKAS_BURN_NS_V1).unwrap();
        let tx_action_benchmark =
            self.tx_action_benchmarks.get_mut(&TxAction::MoneyOtcSwap).unwrap();
        let timer = Instant::now();

        // We're just going to be using a zero spend-hook and user-data
        let rcpt_spend_hook = pallas::Base::zero();
        let rcpt_user_data = pallas::Base::zero();
        let rcpt_user_data_blind = pallas::Base::random(&mut OsRng);

        // Generating  swap blinds
        let value_send_blind = ValueBlind::random(&mut OsRng);
        let value_recv_blind = ValueBlind::random(&mut OsRng);
        let token_send_blind = ValueBlind::random(&mut OsRng);
        let token_recv_blind = ValueBlind::random(&mut OsRng);

        // Builder first holder part
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

        // Builder second holder part
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

        // Then second holder combines the halves
        let swap_full_params = MoneyTransferParamsV1 {
            clear_inputs: vec![],
            inputs: vec![debris0.params.inputs[0].clone(), debris1.params.inputs[0].clone()],
            outputs: vec![debris0.params.outputs[0].clone(), debris1.params.outputs[0].clone()],
        };

        let swap_full_proofs = vec![
            debris0.proofs[0].clone(),
            debris1.proofs[0].clone(),
            debris0.proofs[1].clone(),
            debris1.proofs[1].clone(),
        ];

        // And signs the transaction
        let mut data = vec![MoneyFunction::OtcSwapV1 as u8];
        swap_full_params.encode(&mut data)?;
        let mut tx = Transaction {
            calls: vec![ContractCall { contract_id: *MONEY_CONTRACT_ID, data }],
            proofs: vec![swap_full_proofs],
            signatures: vec![],
        };
        let sigs = tx.create_sigs(&mut OsRng, &[debris1.signature_secret])?;
        tx.signatures = vec![sigs];

        // First holder gets the partially signed transaction and adds their signature
        let sigs = tx.create_sigs(&mut OsRng, &[debris0.signature_secret])?;
        tx.signatures[0].insert(0, sigs[0]);
        tx_action_benchmark.creation_times.push(timer.elapsed());

        // Calculate transaction sizes
        let encoded: Vec<u8> = serialize(&tx);
        let size = std::mem::size_of_val(&*encoded);
        tx_action_benchmark.sizes.push(size);
        let base58 = bs58::encode(&encoded).into_string();
        let size = std::mem::size_of_val(&*base58);
        tx_action_benchmark.broadcasted_sizes.push(size);

        Ok((tx, swap_full_params))
    }

    pub async fn execute_otc_swap_tx(
        &mut self,
        holder: &Holder,
        tx: &Transaction,
        params: &MoneyTransferParamsV1,
        slot: u64,
        append: bool,
    ) -> Result<()> {
        let wallet = self.holders.get_mut(&holder).unwrap();
        let tx_action_benchmark =
            self.tx_action_benchmarks.get_mut(&TxAction::MoneyOtcSwap).unwrap();
        let timer = Instant::now();

        wallet.validator.read().await.add_transactions(&[tx.clone()], slot, true).await?;
        if append {
            for output in &params.outputs {
                wallet.money_merkle_tree.append(MerkleNode::from(output.coin.inner()));
            }
        }
        tx_action_benchmark.verify_times.push(timer.elapsed());

        Ok(())
    }
}
