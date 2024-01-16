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
use darkfi_sdk::{crypto::DEPLOYOOOR_CONTRACT_ID, deploy::DeployParamsV1, ContractCall};
use darkfi_serial::Encodable;
use rand::rngs::OsRng;

use super::{Holder, TestHarness};

impl TestHarness {
    pub fn deploy_contract(
        &mut self,
        holder: &Holder,
        wasm_bincode: Vec<u8>,
    ) -> Result<(Transaction, DeployParamsV1)> {
        let wallet = self.holders.get(holder).unwrap();
        let deploy_keypair = wallet.contract_deploy_authority;

        let (derivecid_pk, derivecid_zkbin) =
            self.proving_keys.get(&DEPLOY_CONTRACT_ZKAS_DERIVE_NS_V1.to_string()).unwrap();

        let builder = DeployCallBuilder {
            deploy_keypair,
            wasm_bincode,
            deploy_ix: vec![],
            derivecid_zkbin: derivecid_zkbin.clone(),
            derivecid_pk: derivecid_pk.clone(),
        };

        let debris = builder.build()?;

        let mut data = vec![DeployFunction::DeployV1 as u8];
        debris.params.encode(&mut data)?;
        let call = ContractCall { contract_id: *DEPLOYOOOR_CONTRACT_ID, data };
        let mut tx_builder =
            TransactionBuilder::new(ContractCallLeaf { call, proofs: debris.proofs }, vec![])?;
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&mut OsRng, &[deploy_keypair.secret])?;
        tx.signatures = vec![sigs];

        Ok((tx, debris.params))
    }

    pub async fn execute_deploy_tx(
        &mut self,
        holder: &Holder,
        tx: &Transaction,
        _params: &DeployParamsV1,
        slot: u64,
    ) -> Result<()> {
        let wallet = self.holders.get_mut(holder).unwrap();

        wallet.validator.add_transactions(&[tx.clone()], slot, true).await?;

        Ok(())
    }
}
