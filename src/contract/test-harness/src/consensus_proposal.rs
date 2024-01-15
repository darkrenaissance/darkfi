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
    client::proposal_v1::ConsensusProposalCallBuilder,
    model::{ConsensusProposalParamsV1, REWARD},
    ConsensusFunction,
};
use darkfi_money_contract::{client::ConsensusOwnCoin, CONSENSUS_CONTRACT_ZKAS_PROPOSAL_NS_V1};
use darkfi_sdk::{
    blockchain::Slot,
    crypto::{MerkleNode, SecretKey, CONSENSUS_CONTRACT_ID},
    ContractCall,
};
use darkfi_serial::{serialize, Encodable};
use log::info;
use rand::rngs::OsRng;

use super::{Holder, TestHarness, TxAction};

impl TestHarness {
    pub async fn proposal(
        &mut self,
        holder: &Holder,
        slot: Slot,
        staked_oc: &ConsensusOwnCoin,
    ) -> Result<(Transaction, ConsensusProposalParamsV1, SecretKey, SecretKey)> {
        let wallet = self.holders.get(holder).unwrap();

        let (proposal_pk, proposal_zkbin) =
            self.proving_keys.get(&CONSENSUS_CONTRACT_ZKAS_PROPOSAL_NS_V1.to_string()).unwrap();

        let tx_action_benchmark =
            self.tx_action_benchmarks.get_mut(&TxAction::ConsensusProposal).unwrap();

        let timer = Instant::now();

        // Proposals always extend genesis block
        let fork_hash = self.genesis_block.hash()?;

        // Building Consensus::Propose params
        let proposal_call_debris = ConsensusProposalCallBuilder {
            owncoin: staked_oc.clone(),
            slot,
            fork_hash,
            fork_previous_hash: fork_hash,
            merkle_tree: wallet.consensus_staked_merkle_tree.clone(),
            proposal_zkbin: proposal_zkbin.clone(),
            proposal_pk: proposal_pk.clone(),
        }
        .build()?;

        let (params, proofs, output_keypair, signature_secret_key) = (
            proposal_call_debris.params,
            proposal_call_debris.proofs,
            proposal_call_debris.keypair,
            proposal_call_debris.signature_secret,
        );

        let mut data = vec![ConsensusFunction::ProposalV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: *CONSENSUS_CONTRACT_ID, data };
        let mut tx_builder = TransactionBuilder::new(ContractCallLeaf { call, proofs }, vec![])?;
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&mut OsRng, &[signature_secret_key])?;
        tx.signatures = vec![sigs];
        tx_action_benchmark.creation_times.push(timer.elapsed());

        // Calculate transaction sizes
        let encoded: Vec<u8> = serialize(&tx);
        let size = std::mem::size_of_val(&*encoded);
        tx_action_benchmark.sizes.push(size);
        let base58 = bs58::encode(&encoded).into_string();
        let size = std::mem::size_of_val(&*base58);
        tx_action_benchmark.broadcasted_sizes.push(size);

        Ok((tx, params, signature_secret_key, output_keypair.secret))
    }

    pub async fn execute_proposal_tx(
        &mut self,
        holder: &Holder,
        tx: &Transaction,
        params: &ConsensusProposalParamsV1,
        slot: u64,
    ) -> Result<()> {
        let wallet = self.holders.get_mut(holder).unwrap();

        let tx_action_benchmark =
            self.tx_action_benchmarks.get_mut(&TxAction::ConsensusProposal).unwrap();

        let timer = Instant::now();

        wallet.validator.add_test_producer_transaction(tx, slot, 2, true).await?;
        wallet.consensus_staked_merkle_tree.append(MerkleNode::from(params.output.coin.inner()));
        tx_action_benchmark.verify_times.push(timer.elapsed());

        Ok(())
    }

    // Execute a proposal transaction and gather rewarded coin
    pub async fn execute_proposal(
        &mut self,
        holders: &[Holder],
        holder: &Holder,
        current_slot: u64,
        slot: Slot,
        staked_oc: &ConsensusOwnCoin,
    ) -> Result<ConsensusOwnCoin> {
        info!(target: "consensus", "[{holder:?}] ====================");
        info!(target: "consensus", "[{holder:?}] Building proposal tx");
        info!(target: "consensus", "[{holder:?}] ====================");
        let (
            proposal_tx,
            proposal_params,
            _proposal_signing_secret_key,
            proposal_decryption_secret_key,
        ) = self.proposal(holder, slot, staked_oc).await?;

        for h in holders {
            info!(target: "consensus", "[{h:?}] ================================");
            info!(target: "consensus", "[{h:?}] Executing {holder:?} proposal tx");
            info!(target: "consensus", "[{h:?}] ================================");
            self.execute_proposal_tx(h, &proposal_tx, &proposal_params, current_slot).await?;
        }

        self.assert_trees(holders);

        // Gather new staked owncoin which includes the reward
        let rewarded_staked_oc = self.gather_consensus_staked_owncoin(
            holder,
            &proposal_params.output,
            Some(proposal_decryption_secret_key),
        )?;

        // Verify values match
        assert!((staked_oc.note.value + REWARD) == rewarded_staked_oc.note.value);

        Ok(rewarded_staked_oc)
    }

    pub async fn execute_erroneous_proposal_tx(
        &mut self,
        holder: &Holder,
        tx: &Transaction,
        slot: u64,
    ) -> Result<()> {
        let wallet = self.holders.get_mut(holder).unwrap();
        let tx_action_benchmark =
            self.tx_action_benchmarks.get_mut(&TxAction::ConsensusProposal).unwrap();
        let timer = Instant::now();

        assert!(wallet.validator.add_test_producer_transaction(tx, slot, 2, true).await.is_err());
        tx_action_benchmark.verify_times.push(timer.elapsed());

        Ok(())
    }
}
