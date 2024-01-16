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

use std::time::Instant;

use darkfi::{
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    Result,
};
use darkfi_money_contract::{
    client::{transfer_v1::make_transfer_call, OwnCoin},
    model::MoneyTransferParamsV1,
    MoneyFunction, MONEY_CONTRACT_ZKAS_BURN_NS_V1, MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};
use darkfi_sdk::{
    crypto::{MerkleNode, TokenId, MONEY_CONTRACT_ID},
    ContractCall,
};
use darkfi_serial::{serialize, Encodable};
use rand::rngs::OsRng;

use super::{Holder, TestHarness, TxAction};

impl TestHarness {
    pub fn transfer(
        &mut self,
        amount: u64,
        holder: &Holder,
        recipient: &Holder,
        owncoins: &[OwnCoin],
        token_id: TokenId,
    ) -> Result<(Transaction, MoneyTransferParamsV1, Vec<OwnCoin>)> {
        let wallet = self.holders.get(holder).unwrap();
        let rcpt = self.holders.get(recipient).unwrap().keypair.public;

        let (mint_pk, mint_zkbin) =
            self.proving_keys.get(&MONEY_CONTRACT_ZKAS_MINT_NS_V1.to_string()).unwrap();

        let (burn_pk, burn_zkbin) =
            self.proving_keys.get(&MONEY_CONTRACT_ZKAS_BURN_NS_V1.to_string()).unwrap();

        let tx_action_benchmark =
            self.tx_action_benchmarks.get_mut(&TxAction::MoneyTransfer).unwrap();

        let timer = Instant::now();

        let (params, secrets, spent_coins) = make_transfer_call(
            wallet.keypair,
            rcpt,
            amount,
            token_id,
            owncoins.to_owned(),
            wallet.money_merkle_tree.clone(),
            mint_zkbin.clone(),
            mint_pk.clone(),
            burn_zkbin.clone(),
            burn_pk.clone(),
        )?;

        let mut data = vec![MoneyFunction::TransferV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };
        let mut tx_builder =
            TransactionBuilder::new(ContractCallLeaf { call, proofs: secrets.proofs }, vec![])?;
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&mut OsRng, &secrets.signature_secrets)?;
        tx.signatures = vec![sigs];
        tx_action_benchmark.creation_times.push(timer.elapsed());

        // Calculate transaction sizes
        let encoded: Vec<u8> = serialize(&tx);
        let size = std::mem::size_of_val(&*encoded);
        tx_action_benchmark.sizes.push(size);
        let base58 = bs58::encode(&encoded).into_string();
        let size = std::mem::size_of_val(&*base58);
        tx_action_benchmark.broadcasted_sizes.push(size);

        Ok((tx, params, spent_coins))
    }

    pub async fn execute_transfer_tx(
        &mut self,
        holder: &Holder,
        tx: &Transaction,
        params: &MoneyTransferParamsV1,
        slot: u64,
        append: bool,
    ) -> Result<()> {
        let wallet = self.holders.get_mut(holder).unwrap();
        let tx_action_benchmark =
            self.tx_action_benchmarks.get_mut(&TxAction::MoneyTransfer).unwrap();
        let timer = Instant::now();

        wallet.validator.add_transactions(&[tx.clone()], slot, true).await?;
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
        holder: &Holder,
        txs: &[Transaction],
        txs_params: &Vec<MoneyTransferParamsV1>,
        slot: u64,
        append: bool,
    ) -> Result<()> {
        let wallet = self.holders.get_mut(holder).unwrap();
        let tx_action_benchmark =
            self.tx_action_benchmarks.get_mut(&TxAction::MoneyTransfer).unwrap();
        let timer = Instant::now();

        wallet.validator.add_transactions(txs, slot, true).await?;
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
        holder: &Holder,
        tx: &Transaction,
        slot: u64,
    ) -> Result<()> {
        let wallet = self.holders.get(holder).unwrap();
        let tx_action_benchmark =
            self.tx_action_benchmarks.get_mut(&TxAction::MoneyTransfer).unwrap();
        let timer = Instant::now();

        wallet.validator.add_transactions(&[tx.clone()], slot, false).await?;
        tx_action_benchmark.verify_times.push(timer.elapsed());

        Ok(())
    }
}
