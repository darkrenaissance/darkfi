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
    client::{transfer_v1::TransferCallBuilder, OwnCoin},
    model::MoneyTransferParamsV1,
    MoneyFunction, MONEY_CONTRACT_ZKAS_BURN_NS_V1, MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};
use darkfi_sdk::{
    crypto::{MerkleNode, TokenId, MONEY_CONTRACT_ID},
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{serialize, Encodable};
use rand::rngs::OsRng;

use super::{Holder, TestHarness, TxAction};

impl TestHarness {
    pub fn transfer(
        &mut self,
        amount: u64,
        holder: Holder,
        recipient: Holder,
        owncoins: &[OwnCoin],
        token_id: TokenId,
    ) -> Result<(Transaction, MoneyTransferParamsV1, Vec<OwnCoin>)> {
        let wallet = self.holders.get(&holder).unwrap();
        let rcpt = self.holders.get(&recipient).unwrap().keypair.public;
        let (mint_pk, mint_zkbin) = self.proving_keys.get(&MONEY_CONTRACT_ZKAS_MINT_NS_V1).unwrap();
        let (burn_pk, burn_zkbin) = self.proving_keys.get(&MONEY_CONTRACT_ZKAS_BURN_NS_V1).unwrap();
        let tx_action_benchmark =
            self.tx_action_benchmarks.get_mut(&TxAction::MoneyTransfer).unwrap();
        let timer = Instant::now();

        // We're just going to be using a zero spend-hook and user-data
        let rcpt_spend_hook = pallas::Base::zero();
        let rcpt_user_data = pallas::Base::zero();
        let rcpt_user_data_blind = pallas::Base::random(&mut OsRng);

        // TODO: verify this is correct
        let change_spend_hook = pallas::Base::zero();
        let change_user_data = pallas::Base::zero();
        let change_user_data_blind = pallas::Base::random(&mut OsRng);

        let builder = TransferCallBuilder {
            keypair: wallet.keypair,
            recipient: rcpt,
            value: amount,
            token_id,
            rcpt_spend_hook,
            rcpt_user_data,
            rcpt_user_data_blind,
            change_spend_hook,
            change_user_data,
            change_user_data_blind,
            coins: owncoins.to_owned(),
            tree: wallet.money_merkle_tree.clone(),
            mint_zkbin: mint_zkbin.clone(),
            mint_pk: mint_pk.clone(),
            burn_zkbin: burn_zkbin.clone(),
            burn_pk: burn_pk.clone(),
            clear_input: false,
        };

        let debris = builder.build()?;

        let mut data = vec![MoneyFunction::TransferV1 as u8];
        debris.params.encode(&mut data)?;
        let calls = vec![ContractCall { contract_id: *MONEY_CONTRACT_ID, data }];
        let proofs = vec![debris.proofs];
        let mut tx = Transaction { calls, proofs, signatures: vec![] };
        let sigs = tx.create_sigs(&mut OsRng, &debris.signature_secrets)?;
        tx.signatures = vec![sigs];
        tx_action_benchmark.creation_times.push(timer.elapsed());

        // Calculate transaction sizes
        let encoded: Vec<u8> = serialize(&tx);
        let size = std::mem::size_of_val(&*encoded);
        tx_action_benchmark.sizes.push(size);
        let base58 = bs58::encode(&encoded).into_string();
        let size = std::mem::size_of_val(&*base58);
        tx_action_benchmark.broadcasted_sizes.push(size);

        Ok((tx, debris.params, debris.spent_coins))
    }

    pub async fn execute_transfer_tx(
        &mut self,
        holder: Holder,
        tx: &Transaction,
        params: &MoneyTransferParamsV1,
        slot: u64,
        append: bool,
    ) -> Result<()> {
        let wallet = self.holders.get_mut(&holder).unwrap();
        let tx_action_benchmark =
            self.tx_action_benchmarks.get_mut(&TxAction::MoneyTransfer).unwrap();
        let timer = Instant::now();

        wallet.validator.read().await.verify_transactions(&[tx.clone()], slot, true).await?;
        if append {
            for output in &params.outputs {
                wallet.money_merkle_tree.append(MerkleNode::from(output.coin.inner()));
            }
        }
        tx_action_benchmark.verify_times.push(timer.elapsed());

        Ok(())
    }

    pub async fn execute_multiple_transfer_txs(
        &mut self,
        holder: Holder,
        txs: &[Transaction],
        txs_params: &Vec<MoneyTransferParamsV1>,
        slot: u64,
        append: bool,
    ) -> Result<()> {
        let wallet = self.holders.get_mut(&holder).unwrap();
        let tx_action_benchmark =
            self.tx_action_benchmarks.get_mut(&TxAction::MoneyTransfer).unwrap();
        let timer = Instant::now();

        wallet.validator.read().await.verify_transactions(txs, slot, true).await?;
        if append {
            for params in txs_params {
                for output in &params.outputs {
                    wallet.money_merkle_tree.append(MerkleNode::from(output.coin.inner()));
                }
            }
        }
        tx_action_benchmark.verify_times.push(timer.elapsed());

        Ok(())
    }

    pub async fn verify_transfer_tx(
        &mut self,
        holder: Holder,
        tx: &Transaction,
        slot: u64,
    ) -> Result<()> {
        let wallet = self.holders.get(&holder).unwrap();
        let tx_action_benchmark =
            self.tx_action_benchmarks.get_mut(&TxAction::MoneyTransfer).unwrap();
        let timer = Instant::now();

        wallet.validator.read().await.verify_transactions(&[tx.clone()], slot, false).await?;
        tx_action_benchmark.verify_times.push(timer.elapsed());

        Ok(())
    }

    pub async fn execute_erroneous_transfer_tx(
        &mut self,
        holder: Holder,
        txs: &[Transaction],
        slot: u64,
        erroneous: usize,
    ) -> Result<()> {
        let wallet = self.holders.get(&holder).unwrap();
        let tx_action_benchmark =
            self.tx_action_benchmarks.get_mut(&TxAction::MoneyTransfer).unwrap();
        let timer = Instant::now();

        let erroneous_txs = wallet
            .validator
            .read()
            .await
            .verify_transactions(txs, slot, false)
            .await
            .err()
            .unwrap()
            .retrieve_erroneous_txs()?;
        assert_eq!(erroneous_txs.len(), erroneous);
        tx_action_benchmark.verify_times.push(timer.elapsed());

        Ok(())
    }
}
