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
    client::genesis_mint_v1::GenesisMintCallBuilder, model::MoneyGenesisMintParamsV1,
    MoneyFunction, MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};
use darkfi_sdk::{
    crypto::{FuncId, MerkleNode, MONEY_CONTRACT_ID},
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{serialize, Encodable};
use rand::rngs::OsRng;

use super::{Holder, TestHarness, TxAction};

impl TestHarness {
    pub fn genesis_mint(
        &mut self,
        holder: &Holder,
        amount: u64,
    ) -> Result<(Transaction, MoneyGenesisMintParamsV1)> {
        let wallet = self.holders.get(holder).unwrap();

        let (mint_pk, mint_zkbin) =
            self.proving_keys.get(&MONEY_CONTRACT_ZKAS_MINT_NS_V1.to_string()).unwrap();

        let tx_action_benchmark =
            self.tx_action_benchmarks.get_mut(&TxAction::MoneyGenesisMint).unwrap();

        let timer = Instant::now();

        // We're just going to be using a zero spend-hook and user-data
        let spend_hook = FuncId::none();
        let user_data = pallas::Base::zero();

        let builder = GenesisMintCallBuilder {
            keypair: wallet.keypair,
            amount,
            spend_hook,
            user_data,
            mint_zkbin: mint_zkbin.clone(),
            mint_pk: mint_pk.clone(),
        };

        let debris = builder.build()?;

        let mut data = vec![MoneyFunction::GenesisMintV1 as u8];
        debris.params.encode(&mut data)?;
        let call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };
        let mut tx_builder =
            TransactionBuilder::new(ContractCallLeaf { call, proofs: debris.proofs }, vec![])?;
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&mut OsRng, &[wallet.keypair.secret])?;
        tx.signatures = vec![sigs];
        tx_action_benchmark.creation_times.push(timer.elapsed());

        // Calculate transaction sizes
        let encoded: Vec<u8> = serialize(&tx);
        let size = std::mem::size_of_val(&*encoded);
        tx_action_benchmark.sizes.push(size);
        let base58 = bs58::encode(&encoded).into_string();
        let size = std::mem::size_of_val(&*base58);
        tx_action_benchmark.broadcasted_sizes.push(size);

        Ok((tx, debris.params))
    }

    pub async fn execute_genesis_mint_tx(
        &mut self,
        holder: &Holder,
        tx: &Transaction,
        params: &MoneyGenesisMintParamsV1,
        block_height: u64,
    ) -> Result<()> {
        let wallet = self.holders.get_mut(holder).unwrap();
        let tx_action_benchmark =
            self.tx_action_benchmarks.get_mut(&TxAction::MoneyGenesisMint).unwrap();
        let timer = Instant::now();

        wallet.validator.add_transactions(&[tx.clone()], block_height, true).await?;
        wallet.money_merkle_tree.append(MerkleNode::from(params.output.coin.inner()));
        tx_action_benchmark.verify_times.push(timer.elapsed());

        Ok(())
    }
}
