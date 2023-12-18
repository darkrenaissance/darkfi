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
    client::unstake_request_v1::ConsensusUnstakeRequestCallBuilder, ConsensusFunction,
};
use darkfi_money_contract::{
    client::ConsensusOwnCoin, model::ConsensusUnstakeReqParamsV1,
    CONSENSUS_CONTRACT_ZKAS_BURN_NS_V1, CONSENSUS_CONTRACT_ZKAS_MINT_NS_V1,
};
use darkfi_sdk::{
    crypto::{MerkleNode, SecretKey, CONSENSUS_CONTRACT_ID},
    ContractCall,
};
use darkfi_serial::{serialize, Encodable};
use log::info;
use rand::rngs::OsRng;

use super::{Holder, TestHarness, TxAction};

impl TestHarness {
    pub async fn unstake_request(
        &mut self,
        holder: &Holder,
        slot: u64,
        staked_oc: &ConsensusOwnCoin,
    ) -> Result<(Transaction, ConsensusUnstakeReqParamsV1, SecretKey, SecretKey)> {
        let wallet = self.holders.get(holder).unwrap();

        let (burn_pk, burn_zkbin) =
            self.proving_keys.get(&CONSENSUS_CONTRACT_ZKAS_BURN_NS_V1.to_string()).unwrap();

        let (mint_pk, mint_zkbin) =
            self.proving_keys.get(&CONSENSUS_CONTRACT_ZKAS_MINT_NS_V1.to_string()).unwrap();

        let tx_action_benchmark =
            self.tx_action_benchmarks.get_mut(&TxAction::ConsensusUnstakeRequest).unwrap();
        let epoch = wallet.validator.consensus.time_keeper.slot_epoch(slot);
        let timer = Instant::now();

        // Building Consensus::Unstake params
        let unstake_request_call_debris = ConsensusUnstakeRequestCallBuilder {
            owncoin: staked_oc.clone(),
            epoch,
            tree: wallet.consensus_staked_merkle_tree.clone(),
            burn_zkbin: burn_zkbin.clone(),
            burn_pk: burn_pk.clone(),
            mint_zkbin: mint_zkbin.clone(),
            mint_pk: mint_pk.clone(),
        }
        .build()?;

        let (
            unstake_request_params,
            unstake_request_proofs,
            unstake_request_output_keypair,
            unstake_request_signature_secret_key,
        ) = (
            unstake_request_call_debris.params,
            unstake_request_call_debris.proofs,
            unstake_request_call_debris.keypair,
            unstake_request_call_debris.signature_secret,
        );

        // Building unstake request tx
        let mut data = vec![ConsensusFunction::UnstakeRequestV1 as u8];
        unstake_request_params.encode(&mut data)?;
        let call = ContractCall { contract_id: *CONSENSUS_CONTRACT_ID, data };
        let mut unstake_request_builder = TransactionBuilder::new(
            ContractCallLeaf { call, proofs: unstake_request_proofs },
            vec![],
        );
        let mut unstake_request_tx = unstake_request_builder.build()?;
        let sigs =
            unstake_request_tx.create_sigs(&mut OsRng, &[unstake_request_signature_secret_key])?;
        unstake_request_tx.signatures = vec![sigs];
        tx_action_benchmark.creation_times.push(timer.elapsed());

        // Calculate transaction sizes
        let encoded: Vec<u8> = serialize(&unstake_request_tx);
        let size = std::mem::size_of_val(&*encoded);
        tx_action_benchmark.sizes.push(size);
        let base58 = bs58::encode(&encoded).into_string();
        let size = std::mem::size_of_val(&*base58);
        tx_action_benchmark.broadcasted_sizes.push(size);

        Ok((
            unstake_request_tx,
            unstake_request_params,
            unstake_request_output_keypair.secret,
            unstake_request_signature_secret_key,
        ))
    }

    pub async fn execute_unstake_request_tx(
        &mut self,
        holder: &Holder,
        tx: &Transaction,
        params: &ConsensusUnstakeReqParamsV1,
        slot: u64,
    ) -> Result<()> {
        let wallet = self.holders.get_mut(holder).unwrap();

        let tx_action_benchmark =
            self.tx_action_benchmarks.get_mut(&TxAction::ConsensusUnstakeRequest).unwrap();

        let timer = Instant::now();

        wallet.validator.add_transactions(&[tx.clone()], slot, true).await?;
        wallet.consensus_unstaked_merkle_tree.append(MerkleNode::from(params.output.coin.inner()));
        tx_action_benchmark.verify_times.push(timer.elapsed());

        Ok(())
    }

    // Execute an unstake request transaction and gather requested unstaked coin
    pub async fn execute_unstake_request(
        &mut self,
        holders: &[Holder],
        holder: &Holder,
        current_slot: u64,
        rewarded_staked_oc: &ConsensusOwnCoin,
    ) -> Result<ConsensusOwnCoin> {
        info!(target: "consensus", "[{holder:?}] ===========================");
        info!(target: "consensus", "[{holder:?}] Building unstake request tx");
        info!(target: "consensus", "[{holder:?}] ===========================");
        let (
            unstake_request_tx,
            unstake_request_params,
            unstake_request_output_secret_key,
            _unstake_request_signature_secret_key,
        ) = self.unstake_request(holder, current_slot, rewarded_staked_oc).await?;

        for h in holders {
            info!(target: "consensus", "[{h:?}] ==================================");
            info!(target: "consensus", "[{h:?}] Executing {holder:?} unstake request tx");
            info!(target: "consensus", "[{h:?}] ==================================");
            self.execute_unstake_request_tx(
                h,
                &unstake_request_tx,
                &unstake_request_params,
                current_slot,
            )
            .await?;
        }

        self.assert_trees(holders);

        // Gather new unstake request owncoin
        let unstake_request_oc = self.gather_consensus_unstaked_owncoin(
            holder,
            &unstake_request_params.output,
            Some(unstake_request_output_secret_key),
        )?;

        // Verify values match
        assert!(rewarded_staked_oc.note.value == unstake_request_oc.note.value);

        Ok(unstake_request_oc)
    }
}
