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
use darkfi_money_contract::{
    client::{burn_v1::make_burn_call, OwnCoin},
    model::{MoneyBurnParamsV1, MoneyFeeParamsV1},
    MoneyFunction, MONEY_CONTRACT_ZKAS_BURN_NS_V1,
};
use darkfi_sdk::{crypto::contract_id::MONEY_CONTRACT_ID, ContractCall};
use darkfi_serial::Encodable;

use super::{Holder, TestHarness};

impl TestHarness {
    /// Create a `Money::Burn` transaction.
    pub async fn burn(
        &mut self,
        holder: &Holder,
        owncoins: &[OwnCoin],
        block_height: u32,
    ) -> Result<(Transaction, (MoneyBurnParamsV1, Option<MoneyFeeParamsV1>), Vec<OwnCoin>)> {
        let wallet = self.wallet(holder);

        let (burn_pk, burn_zkbin) = self.proving_keys.get(MONEY_CONTRACT_ZKAS_BURN_NS_V1).unwrap();

        // Create the burn call
        let (params, secrets, mut spent_coins) = make_burn_call(
            owncoins.to_owned(),
            wallet.money_merkle_tree.clone(),
            burn_zkbin.clone(),
            burn_pk.clone(),
        )?;

        let mut data = vec![MoneyFunction::BurnV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };

        let mut tx_builder =
            TransactionBuilder::new(ContractCallLeaf { call, proofs: secrets.proofs }, vec![])?;

        // Optional fees, if enabled
        let mut fee_params = None;
        let mut fee_signature_secrets = None;
        if self.verify_fees {
            let mut tx = tx_builder.build()?;
            let sigs = tx.create_sigs(&secrets.signature_secrets)?;
            tx.signatures = vec![sigs];

            let (fee_call, fee_proofs, fee_secrets, spent_fee_coins, fee_call_params) =
                self.append_fee_call(holder, tx, block_height, &spent_coins).await?;

            tx_builder.append(ContractCallLeaf { call: fee_call, proofs: fee_proofs }, vec![])?;
            fee_signature_secrets = Some(fee_secrets);
            spent_coins.extend_from_slice(&spent_fee_coins);
            fee_params = Some(fee_call_params);
        }

        // Now build the actual transaction and sign it with all necessary keys
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&secrets.signature_secrets)?;
        tx.signatures = vec![sigs];
        if let Some(fee_signature_secrets) = fee_signature_secrets {
            let sigs = tx.create_sigs(&fee_signature_secrets)?;
            tx.signatures.push(sigs);
        }

        Ok((tx, (params, fee_params), spent_coins))
    }

    /// Execute a `Money::Burn` transaction for a given [`Holder`].
    pub async fn execute_burn_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        call_params: &MoneyBurnParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("money::burn", tx, block_height).await?;

        wallet.process_inputs(&call_params.inputs, holder);

        let mut found_owncoins = vec![];
        if append {
            found_owncoins.extend(wallet.process_fee(fee_params, holder));
        }

        Ok(found_owncoins)
    }
}
