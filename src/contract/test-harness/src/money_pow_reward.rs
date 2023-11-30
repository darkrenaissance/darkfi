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

use darkfi::{tx::Transaction, Result};
use darkfi_money_contract::{
    client::pow_reward_v1::PoWRewardCallBuilder, model::MoneyPoWRewardParamsV1, MoneyFunction,
    MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};
use darkfi_sdk::{
    crypto::{MerkleNode, MONEY_CONTRACT_ID},
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{serialize, Encodable};
use rand::rngs::OsRng;

use super::{Holder, TestHarness, TxAction};

impl TestHarness {
    pub fn pow_reward(
        &mut self,
        holder: &Holder,
        recipient: Option<&Holder>,
        block_height: u64,
        reward: Option<u64>,
    ) -> Result<(Transaction, MoneyPoWRewardParamsV1)> {
        let wallet = self.holders.get(holder).unwrap();

        let (mint_pk, mint_zkbin) =
            self.proving_keys.get(&MONEY_CONTRACT_ZKAS_MINT_NS_V1.to_string()).unwrap();

        let tx_action_benchmark =
            self.tx_action_benchmarks.get_mut(&TxAction::MoneyPoWReward).unwrap();

        let timer = Instant::now();

        // Proposals always extend genesis block
        let last_nonce = self.genesis_block.header.nonce;
        let fork_hash = self.genesis_block.hash()?;

        // We're just going to be using a zero spend-hook and user-data
        let spend_hook = pallas::Base::zero();
        let user_data = pallas::Base::zero();

        let recipient = if let Some(holder) = recipient {
            let holder = self.holders.get(holder).unwrap();
            holder.keypair.public
        } else {
            wallet.keypair.public
        };

        let builder = PoWRewardCallBuilder {
            secret: wallet.keypair.secret,
            recipient,
            block_height,
            last_nonce,
            fork_hash,
            fork_previous_hash: fork_hash,
            spend_hook,
            user_data,
            mint_zkbin: mint_zkbin.clone(),
            mint_pk: mint_pk.clone(),
        };

        let debris = match reward {
            Some(value) => builder.build_with_custom_reward(value)?,
            None => builder.build()?,
        };

        let mut data = vec![MoneyFunction::PoWRewardV1 as u8];
        debris.params.encode(&mut data)?;
        let calls = vec![ContractCall { contract_id: *MONEY_CONTRACT_ID, data }];
        let proofs = vec![debris.proofs];
        let mut tx = Transaction { calls, proofs, signatures: vec![] };
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

    pub async fn execute_pow_reward_tx(
        &mut self,
        holder: &Holder,
        tx: &Transaction,
        params: &MoneyPoWRewardParamsV1,
        block_height: u64,
    ) -> Result<()> {
        let wallet = self.holders.get_mut(holder).unwrap();
        let tx_action_benchmark =
            self.tx_action_benchmarks.get_mut(&TxAction::MoneyPoWReward).unwrap();
        let timer = Instant::now();

        wallet.validator.add_test_producer_transaction(tx, block_height, 1, true).await?;
        wallet.money_merkle_tree.append(MerkleNode::from(params.output.coin.inner()));
        tx_action_benchmark.verify_times.push(timer.elapsed());

        Ok(())
    }

    pub async fn execute_erroneous_pow_reward_tx(
        &mut self,
        holder: &Holder,
        tx: &Transaction,
        block_height: u64,
    ) -> Result<()> {
        let wallet = self.holders.get_mut(holder).unwrap();
        let tx_action_benchmark =
            self.tx_action_benchmarks.get_mut(&TxAction::MoneyPoWReward).unwrap();
        let timer = Instant::now();

        assert!(wallet
            .validator
            .add_test_producer_transaction(tx, block_height, 1, true)
            .await
            .is_err());
        tx_action_benchmark.verify_times.push(timer.elapsed());

        Ok(())
    }
}
