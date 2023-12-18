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
use darkfi_dao_contract::{
    client::{DaoVoteCall, DaoVoteInput},
    model::{Dao, DaoProposal, DaoProposalBulla, DaoVoteParams},
    DaoFunction, DAO_CONTRACT_ZKAS_DAO_VOTE_BURN_NS, DAO_CONTRACT_ZKAS_DAO_VOTE_MAIN_NS,
};
use darkfi_money_contract::client::OwnCoin;
use darkfi_sdk::{
    crypto::{pasta_prelude::Field, Keypair, SecretKey, DAO_CONTRACT_ID},
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{serialize, Encodable};
use rand::rngs::OsRng;

use super::{Holder, TestHarness, TxAction};

impl TestHarness {
    pub fn dao_vote(
        &mut self,
        voter: &Holder,
        dao_kp: &Keypair,
        vote_option: bool,
        dao: &Dao,
        proposal: &DaoProposal,
        proposal_bulla: &DaoProposalBulla,
    ) -> Result<(Transaction, DaoVoteParams)> {
        let wallet = self.holders.get(voter).unwrap();

        let (dao_vote_burn_pk, dao_vote_burn_zkbin) =
            self.proving_keys.get(&DAO_CONTRACT_ZKAS_DAO_VOTE_BURN_NS.to_string()).unwrap();

        let (dao_vote_main_pk, dao_vote_main_zkbin) =
            self.proving_keys.get(&DAO_CONTRACT_ZKAS_DAO_VOTE_MAIN_NS.to_string()).unwrap();

        let tx_action_benchmark = self.tx_action_benchmarks.get_mut(&TxAction::DaoVote).unwrap();
        let timer = Instant::now();

        let (_proposal_leaf_pos, money_merkle_tree) =
            wallet.dao_prop_leafs.get(proposal_bulla).unwrap();

        let vote_owncoin: OwnCoin = wallet
            .unspent_money_coins
            .iter()
            .find(|x| x.note.token_id == dao.gov_token_id)
            .unwrap()
            .clone();

        let signature_secret = SecretKey::random(&mut OsRng);
        let input = DaoVoteInput {
            secret: wallet.keypair.secret,
            note: vote_owncoin.note.clone(),
            leaf_position: vote_owncoin.leaf_position,
            merkle_path: money_merkle_tree.witness(vote_owncoin.leaf_position, 0).unwrap(),
            signature_secret,
        };

        let call = DaoVoteCall {
            inputs: vec![input],
            vote_option,
            yes_vote_blind: pallas::Scalar::random(&mut OsRng),
            vote_keypair: *dao_kp,
            proposal: proposal.clone(),
            dao: dao.clone(),
        };

        let (params, proofs) = call.make(
            dao_vote_burn_zkbin,
            dao_vote_burn_pk,
            dao_vote_main_zkbin,
            dao_vote_main_pk,
        )?;

        let mut data = vec![DaoFunction::Vote as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: *DAO_CONTRACT_ID, data };
        let mut tx_builder = TransactionBuilder::new(ContractCallLeaf { call, proofs }, vec![]);
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&mut OsRng, &[signature_secret])?;
        tx.signatures = vec![sigs];
        tx_action_benchmark.creation_times.push(timer.elapsed());

        // Calculate transaction sizes
        let encoded: Vec<u8> = serialize(&tx);
        let size = std::mem::size_of_val(&*encoded);
        tx_action_benchmark.sizes.push(size);
        let base58 = bs58::encode(&encoded).into_string();
        let size = std::mem::size_of_val(&*base58);
        tx_action_benchmark.broadcasted_sizes.push(size);

        Ok((tx, params))
    }

    pub async fn execute_dao_vote_tx(
        &mut self,
        holder: &Holder,
        tx: &Transaction,
        _params: &DaoVoteParams,
        slot: u64,
    ) -> Result<()> {
        let wallet = self.holders.get_mut(holder).unwrap();
        let tx_action_benchmark = self.tx_action_benchmarks.get_mut(&TxAction::DaoVote).unwrap();
        let timer = Instant::now();

        wallet.validator.add_transactions(&[tx.clone()], slot, true).await?;

        tx_action_benchmark.verify_times.push(timer.elapsed());

        Ok(())
    }
}
