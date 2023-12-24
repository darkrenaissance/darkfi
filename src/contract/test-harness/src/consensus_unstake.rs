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

use darkfi::{
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    Result,
};
use darkfi_consensus_contract::{
    client::unstake_v1::ConsensusUnstakeCallBuilder, ConsensusFunction,
};
use darkfi_money_contract::{
    client::{unstake_v1::MoneyUnstakeCallBuilder, ConsensusOwnCoin, OwnCoin},
    model::MoneyUnstakeParamsV1,
    MoneyFunction, CONSENSUS_CONTRACT_ZKAS_BURN_NS_V1, MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};
use darkfi_sdk::{
    crypto::{MerkleNode, SecretKey, CONSENSUS_CONTRACT_ID, MONEY_CONTRACT_ID},
    dark_tree::DarkTree,
    ContractCall,
};
use darkfi_serial::{serialize, Encodable};
use log::info;
use rand::rngs::OsRng;

use super::{Holder, TestHarness, TxAction};

impl TestHarness {
    pub fn unstake(
        &mut self,
        holder: &Holder,
        staked_oc: &ConsensusOwnCoin,
    ) -> Result<(Transaction, MoneyUnstakeParamsV1, SecretKey)> {
        let wallet = self.holders.get(holder).unwrap();

        let (burn_pk, burn_zkbin) =
            self.proving_keys.get(&CONSENSUS_CONTRACT_ZKAS_BURN_NS_V1.to_string()).unwrap();
        let (mint_pk, mint_zkbin) =
            self.proving_keys.get(&MONEY_CONTRACT_ZKAS_MINT_NS_V1.to_string()).unwrap();

        let tx_action_benchmark =
            self.tx_action_benchmarks.get_mut(&TxAction::ConsensusUnstake).unwrap();

        let timer = Instant::now();

        // Building Consensus::Unstake params
        let consensus_unstake_call_debris = ConsensusUnstakeCallBuilder {
            owncoin: staked_oc.clone(),
            tree: wallet.consensus_unstaked_merkle_tree.clone(),
            burn_zkbin: burn_zkbin.clone(),
            burn_pk: burn_pk.clone(),
        }
        .build()?;
        let (
            consensus_unstake_params,
            consensus_unstake_proofs,
            consensus_unstake_secret_key,
            consensus_unstake_value_blind,
        ) = (
            consensus_unstake_call_debris.params,
            consensus_unstake_call_debris.proofs,
            consensus_unstake_call_debris.signature_secret,
            consensus_unstake_call_debris.value_blind,
        );

        // Building Money::Unstake params
        let money_unstake_call_debris = MoneyUnstakeCallBuilder {
            owncoin: staked_oc.clone(),
            recipient: self.holders.get(holder).unwrap().keypair.public,
            value_blind: consensus_unstake_value_blind,
            nullifier: consensus_unstake_params.input.nullifier,
            merkle_root: consensus_unstake_params.input.merkle_root,
            signature_public: consensus_unstake_params.input.signature_public,
            mint_zkbin: mint_zkbin.clone(),
            mint_pk: mint_pk.clone(),
        }
        .build()?;
        let (money_unstake_params, money_unstake_proofs) =
            (money_unstake_call_debris.params, money_unstake_call_debris.proofs);

        // Building unstake tx
        let mut data = vec![ConsensusFunction::UnstakeV1 as u8];
        consensus_unstake_params.encode(&mut data)?;
        let consensus_call = ContractCall { contract_id: *CONSENSUS_CONTRACT_ID, data };

        let mut data = vec![MoneyFunction::UnstakeV1 as u8];
        money_unstake_params.encode(&mut data)?;
        let money_call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };

        let mut unstake_tx_builder = TransactionBuilder::new(
            ContractCallLeaf { call: money_call, proofs: money_unstake_proofs },
            vec![DarkTree::new(
                ContractCallLeaf { call: consensus_call, proofs: consensus_unstake_proofs },
                vec![],
                None,
                None,
            )],
        )?;
        let mut unstake_tx = unstake_tx_builder.build()?;
        let consensus_sigs = unstake_tx.create_sigs(&mut OsRng, &[consensus_unstake_secret_key])?;
        let money_sigs = unstake_tx.create_sigs(&mut OsRng, &[consensus_unstake_secret_key])?;
        unstake_tx.signatures = vec![consensus_sigs, money_sigs];
        tx_action_benchmark.creation_times.push(timer.elapsed());

        // Calculate transaction sizes
        let encoded: Vec<u8> = serialize(&unstake_tx);
        let size = ::std::mem::size_of_val(&*encoded);
        tx_action_benchmark.sizes.push(size);
        let base58 = bs58::encode(&encoded).into_string();
        let size = ::std::mem::size_of_val(&*base58);
        tx_action_benchmark.broadcasted_sizes.push(size);

        Ok((unstake_tx, money_unstake_params, consensus_unstake_secret_key))
    }

    pub async fn execute_unstake_tx(
        &mut self,
        holder: &Holder,
        tx: &Transaction,
        params: &MoneyUnstakeParamsV1,
        slot: u64,
    ) -> Result<()> {
        let wallet = self.holders.get_mut(holder).unwrap();
        let tx_action_benchmark =
            self.tx_action_benchmarks.get_mut(&TxAction::ConsensusUnstake).unwrap();
        let timer = Instant::now();

        wallet.validator.add_transactions(&[tx.clone()], slot, true).await?;
        wallet.money_merkle_tree.append(MerkleNode::from(params.output.coin.inner()));
        tx_action_benchmark.verify_times.push(timer.elapsed());

        Ok(())
    }

    // Execute an unstake transaction and gather unstaked coin
    pub async fn execute_unstake(
        &mut self,
        holders: &[Holder],
        holder: &Holder,
        current_slot: u64,
        unstake_request_oc: &ConsensusOwnCoin,
    ) -> Result<OwnCoin> {
        info!(target: "consensus", "[{holder:?}] ===================");
        info!(target: "consensus", "[{holder:?}] Building unstake tx");
        info!(target: "consensus", "[{holder:?}] ===================");
        let (unstake_tx, unstake_params, _) = self.unstake(holder, unstake_request_oc)?;

        for h in holders {
            info!(target: "consensus", "[{h:?}] ===============================");
            info!(target: "consensus", "[{h:?}] Executing {holder:?} unstake tx");
            info!(target: "consensus", "[{h:?}] ===============================");
            self.execute_unstake_tx(h, &unstake_tx, &unstake_params, current_slot).await?;
        }

        self.assert_trees(holders);

        // Gather new unstaked owncoin
        let unstaked_oc = self.gather_owncoin(holder, &unstake_params.output, None)?;

        // Verify values match
        assert!(unstake_request_oc.note.value == unstaked_oc.note.value);

        Ok(unstaked_oc)
    }
}
