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
use darkfi_deployooor_contract::{
    client::deploy_v1::DeployCallBuilder, DeployFunction, DEPLOY_CONTRACT_ZKAS_DERIVE_NS_V1,
};
use darkfi_money_contract::{
    client::{MoneyNote, OwnCoin},
    model::MoneyFeeParamsV1,
};
use darkfi_sdk::{
    crypto::{contract_id::DEPLOYOOOR_CONTRACT_ID, MerkleNode},
    deploy::DeployParamsV1,
    ContractCall,
};
use darkfi_serial::AsyncEncodable;
use log::debug;

use super::{Holder, TestHarness};

impl TestHarness {
    /// Create a `Deployooor::Deploy` transaction with the given WASM bincode.
    ///
    /// Returns the [`Transaction`], and necessary parameters.
    pub async fn deploy_contract(
        &mut self,
        holder: &Holder,
        wasm_bincode: Vec<u8>,
        block_height: u64,
    ) -> Result<(Transaction, DeployParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.holders.get(holder).unwrap();
        let deploy_keypair = wallet.contract_deploy_authority;

        let (derivecid_pk, derivecid_zkbin) =
            self.proving_keys.get(&DEPLOY_CONTRACT_ZKAS_DERIVE_NS_V1.to_string()).unwrap();

        // Build the contract call
        let builder = DeployCallBuilder {
            deploy_keypair,
            wasm_bincode,
            deploy_ix: vec![],
            derivecid_zkbin: derivecid_zkbin.clone(),
            derivecid_pk: derivecid_pk.clone(),
        };
        let debris = builder.build()?;

        // Encode the call
        let mut data = vec![DeployFunction::DeployV1 as u8];
        debris.params.encode_async(&mut data).await?;
        let call = ContractCall { contract_id: *DEPLOYOOOR_CONTRACT_ID, data };
        let mut tx_builder =
            TransactionBuilder::new(ContractCallLeaf { call, proofs: debris.proofs }, vec![])?;

        // If we have tx fees enabled, make an offering
        let mut fee_params = None;
        let mut fee_signature_secrets = None;
        if self.verify_fees {
            let mut tx = tx_builder.build()?;
            let sigs = tx.create_sigs(&[deploy_keypair.secret])?;
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
        let sigs = tx.create_sigs(&[deploy_keypair.secret])?;
        tx.signatures = vec![sigs];
        if let Some(fee_signature_secrets) = fee_signature_secrets {
            let sigs = tx.create_sigs(&fee_signature_secrets)?;
            tx.signatures.push(sigs);
        }

        Ok((tx, debris.params, fee_params))
    }

    /// Execute the transaction created by `deploy_contract()` for a given [`Holder`].
    ///
    /// Returns any found [`OwnCoin`]s.
    pub async fn execute_deploy_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &DeployParamsV1,
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

            debug!("Found new OwnCoin({}) for {:?}", owncoin.coin, holder);
            wallet.unspent_money_coins.push(owncoin.clone());
            return Ok(vec![owncoin])
        }

        Ok(vec![])
    }
}
