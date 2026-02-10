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
    client::make_mint_call,
    model::{Dao, DaoMintParams},
    DaoFunction, DAO_CONTRACT_ZKAS_MINT_NS,
};
use darkfi_money_contract::{client::OwnCoin, model::MoneyFeeParamsV1};
use darkfi_sdk::{
    crypto::{contract_id::DAO_CONTRACT_ID, MerkleNode, SecretKey},
    ContractCall,
};
use darkfi_serial::Encodable;

use super::{Holder, TestHarness};

impl TestHarness {
    /// Create a `Dao::Mint` transaction with the given [`Dao`] info and keys.
    /// Takes a [`Holder`] for optionally paying the transaction fee.
    ///
    /// Returns the [`Transaction`] and any relevant parameters.
    #[allow(clippy::too_many_arguments)]
    pub async fn dao_mint(
        &mut self,
        holder: &Holder,
        dao: &Dao,
        dao_notes_secret_key: &SecretKey,
        dao_proposer_secret_key: &SecretKey,
        dao_proposals_secret_key: &SecretKey,
        dao_votes_secret_key: &SecretKey,
        dao_exec_secret_key: &SecretKey,
        dao_early_exec_secret_key: &SecretKey,
        block_height: u32,
    ) -> Result<(Transaction, DaoMintParams, Option<MoneyFeeParamsV1>)> {
        let (dao_mint_pk, dao_mint_zkbin) =
            self.proving_keys.get(DAO_CONTRACT_ZKAS_MINT_NS).unwrap();

        // Create the call
        let (params, proofs) = make_mint_call(
            dao,
            dao_notes_secret_key,
            dao_proposer_secret_key,
            dao_proposals_secret_key,
            dao_votes_secret_key,
            dao_exec_secret_key,
            dao_early_exec_secret_key,
            dao_mint_zkbin,
            dao_mint_pk,
        )?;

        // Encode the call
        let mut data = vec![DaoFunction::Mint as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: *DAO_CONTRACT_ID, data };
        let mut tx_builder = TransactionBuilder::new(ContractCallLeaf { call, proofs }, vec![])?;

        // If fees are enabled, make an offering
        let mut fee_params = None;
        let mut fee_signature_secrets = None;
        if self.verify_fees {
            let mut tx = tx_builder.build()?;
            let sigs = tx.create_sigs(&[*dao_notes_secret_key])?;
            tx.signatures = vec![sigs];

            let (fee_call, fee_proofs, fee_secrets, _spent_fee_coins, fee_call_params) =
                self.append_fee_call(holder, tx, block_height, &[]).await?;

            // Append the fee call to the transaction
            tx_builder.append(ContractCallLeaf { call: fee_call, proofs: fee_proofs }, vec![])?;
            fee_signature_secrets = Some(fee_secrets);
            fee_params = Some(fee_call_params);
        }

        // Now build the actual transaction and sign it with necessary keys.
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&[*dao_notes_secret_key])?;
        tx.signatures = vec![sigs];
        if let Some(fee_signature_secrets) = fee_signature_secrets {
            let sigs = tx.create_sigs(&fee_signature_secrets)?;
            tx.signatures.push(sigs);
        }

        Ok((tx, params, fee_params))
    }

    /// Execute the transaction created by `dao_mint()` for a given [`Holder`].
    ///
    /// Returns any found [`OwnCoin`]s.
    pub async fn execute_dao_mint_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        params: &DaoMintParams,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("dao::mint", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        // Track the DAO bulla in the DAO Merkle tree
        wallet.dao_merkle_tree.append(MerkleNode::from(params.dao_bulla.inner()));
        let leaf_pos = wallet.dao_merkle_tree.mark().unwrap();
        wallet.dao_leafs.insert(params.dao_bulla, leaf_pos);

        Ok(wallet.process_fee(fee_params, holder))
    }
}
