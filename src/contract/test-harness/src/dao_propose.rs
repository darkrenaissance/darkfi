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
    client::{DaoProposeCall, DaoProposeStakeInput},
    model::{Dao, DaoAuthCall, DaoBulla, DaoProposal, DaoProposeParams},
    slot_to_day, DaoFunction, DAO_CONTRACT_ZKAS_DAO_PROPOSE_BURN_NS,
    DAO_CONTRACT_ZKAS_DAO_PROPOSE_MAIN_NS,
};
use darkfi_money_contract::{client::OwnCoin, model::CoinAttributes, MoneyFunction};
use darkfi_sdk::{
    crypto::{pasta_prelude::Field, MerkleNode, SecretKey, DAO_CONTRACT_ID, MONEY_CONTRACT_ID},
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{serialize, Encodable};
use rand::rngs::OsRng;

use super::{Holder, TestHarness, TxAction};

impl TestHarness {
    pub fn dao_propose(
        &mut self,
        proposer: &Holder,
        proposal_coinattrs: &Vec<CoinAttributes>,
        user_data: pallas::Base,
        dao: &Dao,
        dao_bulla: &DaoBulla,
    ) -> Result<(Transaction, DaoProposeParams, DaoProposal)> {
        let wallet = self.holders.get(proposer).unwrap();

        let (dao_propose_burn_pk, dao_propose_burn_zkbin) =
            self.proving_keys.get(&DAO_CONTRACT_ZKAS_DAO_PROPOSE_BURN_NS.to_string()).unwrap();
        let (dao_propose_main_pk, dao_propose_main_zkbin) =
            self.proving_keys.get(&DAO_CONTRACT_ZKAS_DAO_PROPOSE_MAIN_NS.to_string()).unwrap();

        let tx_action_benchmark = self.tx_action_benchmarks.get_mut(&TxAction::DaoPropose).unwrap();
        let timer = Instant::now();

        let propose_owncoin: OwnCoin = wallet
            .unspent_money_coins
            .iter()
            .find(|x| x.note.token_id == dao.gov_token_id)
            .unwrap()
            .clone();

        let signature_secret = SecretKey::random(&mut OsRng);
        let input = DaoProposeStakeInput {
            secret: wallet.keypair.secret,
            note: propose_owncoin.note.clone(),
            leaf_position: propose_owncoin.leaf_position,
            merkle_path: wallet
                .money_merkle_tree
                .witness(propose_owncoin.leaf_position, 0)
                .unwrap(),
            signature_secret,
        };

        // Convert coin_params to actual coins
        let mut proposal_coins = vec![];
        for coin_params in proposal_coinattrs {
            proposal_coins.push(coin_params.to_coin());
        }
        let mut proposal_data = vec![];
        proposal_coins.encode(&mut proposal_data).unwrap();

        let auth_calls = vec![
            DaoAuthCall {
                contract_id: DAO_CONTRACT_ID.inner(),
                function_code: DaoFunction::AuthMoneyTransfer as u8,
                auth_data: proposal_data,
            },
            DaoAuthCall {
                contract_id: MONEY_CONTRACT_ID.inner(),
                function_code: MoneyFunction::TransferV1 as u8,
                auth_data: vec![],
            },
        ];

        let creation_day = slot_to_day(wallet.validator.consensus.time_keeper.verifying_slot);
        let proposal = DaoProposal {
            auth_calls,
            creation_day,
            duration_days: 30,
            user_data,
            dao_bulla: dao.to_bulla(),
            blind: pallas::Base::random(&mut OsRng),
        };

        let call = DaoProposeCall {
            inputs: vec![input],
            proposal: proposal.clone(),
            dao: dao.clone(),
            dao_leaf_position: *wallet.dao_leafs.get(dao_bulla).unwrap(),
            dao_merkle_path: wallet
                .dao_merkle_tree
                .witness(*wallet.dao_leafs.get(dao_bulla).unwrap(), 0)
                .unwrap(),
            dao_merkle_root: wallet.dao_merkle_tree.root(0).unwrap(),
        };

        let (params, proofs) = call.make(
            dao_propose_burn_zkbin,
            dao_propose_burn_pk,
            dao_propose_main_zkbin,
            dao_propose_main_pk,
        )?;

        let mut data = vec![DaoFunction::Propose as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: *DAO_CONTRACT_ID, data };
        let mut tx_builder = TransactionBuilder::new(ContractCallLeaf { call, proofs }, vec![])?;
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

        Ok((tx, params, proposal))
    }

    pub async fn execute_dao_propose_tx(
        &mut self,
        holder: &Holder,
        tx: &Transaction,
        params: &DaoProposeParams,
        slot: u64,
    ) -> Result<()> {
        let wallet = self.holders.get_mut(holder).unwrap();
        let tx_action_benchmark = self.tx_action_benchmarks.get_mut(&TxAction::DaoPropose).unwrap();
        let timer = Instant::now();

        wallet.validator.add_transactions(&[tx.clone()], slot, true).await?;
        wallet.dao_proposals_tree.append(MerkleNode::from(params.proposal_bulla.inner()));

        let prop_leaf_pos = wallet.dao_proposals_tree.mark().unwrap();
        let prop_money_snapshot = wallet.money_merkle_tree.clone();

        wallet.dao_prop_leafs.insert(params.proposal_bulla, (prop_leaf_pos, prop_money_snapshot));

        tx_action_benchmark.verify_times.push(timer.elapsed());

        Ok(())
    }
}
