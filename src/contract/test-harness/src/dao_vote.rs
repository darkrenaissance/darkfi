/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use darkfi::{
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    Result,
};
use darkfi_dao_contract::{
    blockwindow,
    client::{DaoVoteCall, DaoVoteInput},
    model::{Dao, DaoProposal, DaoVoteParams},
    DaoFunction, DAO_CONTRACT_ZKAS_VOTE_INPUT_NS, DAO_CONTRACT_ZKAS_VOTE_MAIN_NS,
};
use darkfi_money_contract::{client::OwnCoin, model::MoneyFeeParamsV1};
use darkfi_sdk::{crypto::contract_id::DAO_CONTRACT_ID, ContractCall};
use darkfi_serial::Encodable;

use super::{Holder, TestHarness};

impl TestHarness {
    /// Create a `Dao::Vote` transaction.
    pub async fn dao_vote(
        &mut self,
        voter: &Holder,
        vote_option: bool,
        dao: &Dao,
        proposal: &DaoProposal,
        block_height: u32,
    ) -> Result<(Transaction, DaoVoteParams, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(voter);

        let (dao_vote_burn_pk, dao_vote_burn_zkbin) =
            self.proving_keys.get(DAO_CONTRACT_ZKAS_VOTE_INPUT_NS).unwrap();

        let (dao_vote_main_pk, dao_vote_main_zkbin) =
            self.proving_keys.get(DAO_CONTRACT_ZKAS_VOTE_MAIN_NS).unwrap();

        let (_, snapshot_money_merkle_tree) =
            wallet.dao_prop_leafs.get(&proposal.to_bulla()).unwrap();

        let vote_owncoin: OwnCoin = wallet
            .unspent_money_coins
            .iter()
            .find(|x| x.note.token_id == dao.gov_token_id)
            .unwrap()
            .clone();

        let input = DaoVoteInput {
            secret: wallet.keypair.secret,
            note: vote_owncoin.note.clone(),
            leaf_position: vote_owncoin.leaf_position,
            merkle_path: snapshot_money_merkle_tree.witness(vote_owncoin.leaf_position, 0).unwrap(),
        };

        let block_target = wallet.validator.read().await.consensus.module.target;
        let current_blockwindow = blockwindow(block_height, block_target);
        let call = DaoVoteCall {
            money_null_smt: wallet.money_null_smt_snapshot.as_ref().unwrap(),
            inputs: vec![input],
            vote_option,
            proposal: proposal.clone(),
            dao: dao.clone(),
            current_blockwindow,
        };

        let (params, proofs, signature_secrets) = call.make(
            dao_vote_burn_zkbin,
            dao_vote_burn_pk,
            dao_vote_main_zkbin,
            dao_vote_main_pk,
        )?;

        // Encode the call
        let mut data = vec![DaoFunction::Vote as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: *DAO_CONTRACT_ID, data };
        let mut tx_builder = TransactionBuilder::new(ContractCallLeaf { call, proofs }, vec![])?;

        // If fees are enabled, make an offering
        let mut fee_params = None;
        let mut fee_signature_secrets = None;
        if self.verify_fees {
            let mut tx = tx_builder.build()?;
            let sigs = tx.create_sigs(&signature_secrets)?;
            tx.signatures.push(sigs);

            let (fee_call, fee_proofs, fee_secrets, _spent_fee_coins, fee_call_params) =
                self.append_fee_call(voter, tx, block_height, &[]).await?;

            // Append the fee call to the transaction
            tx_builder.append(ContractCallLeaf { call: fee_call, proofs: fee_proofs }, vec![])?;
            fee_signature_secrets = Some(fee_secrets);
            fee_params = Some(fee_call_params);
        }

        // Now build the actual transaction and sign it with necessary keys.
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&signature_secrets)?;
        tx.signatures.push(sigs);
        if let Some(fee_signature_secrets) = fee_signature_secrets {
            let sigs = tx.create_sigs(&fee_signature_secrets)?;
            tx.signatures.push(sigs);
        }

        Ok((tx, params, fee_params))
    }

    /// Execute the transaction made by `dao_vote()` for a given [`Holder`].
    ///
    /// Returns any found [`OwnCoin`]s.
    pub async fn execute_dao_vote_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("dao::vote", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }
}
