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

use darkfi::{
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    Result,
};
use darkfi_dao_contract::{
    blockwindow,
    client::{DaoProposeCall, DaoProposeStakeInput},
    model::{Dao, DaoAuthCall, DaoBulla, DaoProposal, DaoProposeParams},
    DaoFunction, DAO_CONTRACT_ZKAS_DAO_PROPOSE_INPUT_NS, DAO_CONTRACT_ZKAS_DAO_PROPOSE_MAIN_NS,
};
use darkfi_money_contract::{
    client::{MoneyNote, OwnCoin},
    model::{CoinAttributes, MoneyFeeParamsV1},
    MoneyFunction,
};
use darkfi_sdk::{
    crypto::{
        contract_id::{DAO_CONTRACT_ID, MONEY_CONTRACT_ID},
        Blind, MerkleNode, SecretKey,
    },
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::AsyncEncodable;
use log::debug;
use rand::rngs::OsRng;

use super::{Holder, TestHarness};

impl TestHarness {
    /// Create a `Dao::Propose` transaction.
    pub async fn dao_propose(
        &mut self,
        proposer: &Holder,
        proposal_coinattrs: &[CoinAttributes],
        user_data: pallas::Base,
        dao: &Dao,
        dao_bulla: &DaoBulla,
        block_height: u64,
    ) -> Result<(Transaction, (DaoProposeParams, Option<MoneyFeeParamsV1>), DaoProposal)> {
        let wallet = self.holders.get(proposer).unwrap();

        let (dao_propose_burn_pk, dao_propose_burn_zkbin) =
            self.proving_keys.get(&DAO_CONTRACT_ZKAS_DAO_PROPOSE_INPUT_NS.to_string()).unwrap();

        let (dao_propose_main_pk, dao_propose_main_zkbin) =
            self.proving_keys.get(&DAO_CONTRACT_ZKAS_DAO_PROPOSE_MAIN_NS.to_string()).unwrap();

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
        proposal_coins.encode_async(&mut proposal_data).await?;

        // Create Auth calls
        let auth_calls = vec![
            DaoAuthCall {
                contract_id: *DAO_CONTRACT_ID,
                function_code: DaoFunction::AuthMoneyTransfer as u8,
                auth_data: proposal_data,
            },
            DaoAuthCall {
                contract_id: *MONEY_CONTRACT_ID,
                function_code: MoneyFunction::TransferV1 as u8,
                auth_data: vec![],
            },
        ];

        let creation_day = blockwindow(block_height);
        let proposal = DaoProposal {
            auth_calls,
            creation_day,
            duration_days: 30,
            user_data,
            dao_bulla: dao.to_bulla(),
            blind: Blind::random(&mut OsRng),
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

        // Encode the call
        let mut data = vec![DaoFunction::Propose as u8];
        params.encode_async(&mut data).await?;
        let call = ContractCall { contract_id: *DAO_CONTRACT_ID, data };
        let mut tx_builder = TransactionBuilder::new(ContractCallLeaf { call, proofs }, vec![])?;

        // If fees are enabled, make an offering
        let mut fee_params = None;
        let mut fee_signature_secrets = None;
        if self.verify_fees {
            let mut tx = tx_builder.build()?;
            let sigs = tx.create_sigs(&[signature_secret])?;
            tx.signatures = vec![sigs];

            let (fee_call, fee_proofs, fee_secrets, _spent_fee_coins, fee_call_params) =
                self.append_fee_call(proposer, tx, block_height, &[]).await?;

            // Append the fee call to the transaction
            tx_builder.append(ContractCallLeaf { call: fee_call, proofs: fee_proofs }, vec![])?;
            fee_signature_secrets = Some(fee_secrets);
            fee_params = Some(fee_call_params);
        }

        // Now build the actual transaction and sign it with necessary keys.
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&[signature_secret])?;
        tx.signatures = vec![sigs];
        if let Some(fee_signature_secrets) = fee_signature_secrets {
            let sigs = tx.create_sigs(&fee_signature_secrets)?;
            tx.signatures.push(sigs);
        }

        Ok((tx, (params, fee_params), proposal))
    }

    /// Execute the transaction created by `dao_propose()` for a given [`Holder`].
    ///
    /// Returns any found [`OwnCoin`]s.
    pub async fn execute_dao_propose_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        params: &DaoProposeParams,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u64,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.holders.get_mut(holder).unwrap();

        // Execute the transaction
        wallet.validator.add_transactions(&[tx], block_height, true, self.verify_fees).await?;

        if !append {
            return Ok(vec![])
        }

        wallet.dao_proposals_tree.append(MerkleNode::from(params.proposal_bulla.inner()));
        let prop_leaf_pos = wallet.dao_proposals_tree.mark().unwrap();
        let prop_money_snapshot = wallet.money_merkle_tree.clone();
        wallet.dao_prop_leafs.insert(params.proposal_bulla, (prop_leaf_pos, prop_money_snapshot));

        if let Some(ref fee_params) = fee_params {
            if let Some(spent_coin) = wallet
                .unspent_money_coins
                .iter()
                .find(|x| x.nullifier() == fee_params.input.nullifier)
                .cloned()
            {
                debug!("Found spent OwnCoin({}) for {:?}", spent_coin.coin, holder);
                wallet.unspent_money_coins.retain(|x| x.nullifier() != fee_params.input.nullifier);
                wallet.spent_money_coins.push(spent_coin.clone());
            }

            wallet.money_merkle_tree.append(MerkleNode::from(fee_params.output.coin.inner()));

            let Ok(note) = fee_params.output.note.decrypt::<MoneyNote>(&wallet.keypair.secret)
            else {
                return Ok(vec![])
            };

            let owncoin = OwnCoin {
                coin: fee_params.output.coin,
                note: note.clone(),
                secret: wallet.keypair.secret,
                leaf_position: wallet.money_merkle_tree.mark().unwrap(),
            };

            debug!("Found new OwnCoin({}) for {:?}:", owncoin.coin, holder);
            wallet.unspent_money_coins.push(owncoin.clone());
            return Ok(vec![owncoin])
        }

        Ok(vec![])
    }
}
