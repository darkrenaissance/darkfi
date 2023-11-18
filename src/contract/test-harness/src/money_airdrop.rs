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

use darkfi::{tx::Transaction, zk::halo2::Field, Result};
use darkfi_money_contract::{
    client::{transfer_v1 as xfer, OwnCoin},
    model::MoneyTransferParamsV1,
    MoneyFunction, MONEY_CONTRACT_ZKAS_BURN_NS_V1, MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};
use darkfi_sdk::{
    crypto::{MerkleNode, DARK_TOKEN_ID, MONEY_CONTRACT_ID},
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{serialize, Encodable};
use log::info;
use rand::rngs::OsRng;

use super::{Holder, TestHarness, TxAction};

impl TestHarness {
    pub fn airdrop_native(
        &mut self,
        value: u64,
        holder: &Holder,
        rcpt_spend_hook: Option<pallas::Base>,
        rcpt_user_data: Option<pallas::Base>,
    ) -> Result<(Transaction, MoneyTransferParamsV1)> {
        let recipient = self.holders.get(holder).unwrap().keypair.public;
        let faucet = self.holders.get(&Holder::Faucet).unwrap();

        let (mint_pk, mint_zkbin) =
            self.proving_keys.get(&MONEY_CONTRACT_ZKAS_MINT_NS_V1.to_string()).unwrap();

        let (burn_pk, burn_zkbin) =
            self.proving_keys.get(&MONEY_CONTRACT_ZKAS_BURN_NS_V1.to_string()).unwrap();

        let tx_action_benchmark =
            self.tx_action_benchmarks.get_mut(&TxAction::MoneyAirdrop).unwrap();

        let timer = Instant::now();

        let xfer_builder = xfer::TransferCallBuilder {
            clear_inputs: vec![xfer::TransferCallClearInput {
                value,
                token_id: *DARK_TOKEN_ID,
                signature_secret: faucet.keypair.secret,
            }],
            inputs: vec![],
            outputs: vec![xfer::TransferCallOutput {
                value,
                token_id: *DARK_TOKEN_ID,
                public_key: recipient,
                spend_hook: rcpt_spend_hook.unwrap_or(pallas::Base::ZERO),
                user_data: rcpt_user_data.unwrap_or(pallas::Base::ZERO),
            }],
            mint_zkbin: mint_zkbin.clone(),
            mint_pk: mint_pk.clone(),
            burn_zkbin: burn_zkbin.clone(),
            burn_pk: burn_pk.clone(),
        };

        let (params, secrets) = xfer_builder.build()?;

        let mut data = vec![MoneyFunction::TransferV1 as u8];
        params.encode(&mut data)?;
        let calls = vec![ContractCall { contract_id: *MONEY_CONTRACT_ID, data }];
        let proofs = vec![secrets.proofs];
        let mut tx = Transaction { calls, proofs, signatures: vec![] };
        let sigs = tx.create_sigs(&mut OsRng, &secrets.signature_secrets)?;
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

    pub async fn execute_airdrop_native_tx(
        &mut self,
        holder: &Holder,
        tx: &Transaction,
        params: &MoneyTransferParamsV1,
        slot: u64,
    ) -> Result<()> {
        let wallet = self.holders.get_mut(holder).unwrap();
        let tx_action_benchmark =
            self.tx_action_benchmarks.get_mut(&TxAction::MoneyAirdrop).unwrap();
        let timer = Instant::now();

        wallet.validator.read().await.add_transactions(&[tx.clone()], slot, true).await?;
        wallet.money_merkle_tree.append(MerkleNode::from(params.outputs[0].coin.inner()));
        tx_action_benchmark.verify_times.push(timer.elapsed());

        Ok(())
    }

    // Execute an airdrop transaction and gather the coin
    pub async fn execute_airdrop(
        &mut self,
        holders: &[Holder],
        holder: &Holder,
        value: u64,
        current_slot: u64,
    ) -> Result<OwnCoin> {
        info!(target: "consensus", "[Faucet] ==============================");
        info!(target: "consensus", "[Faucet] Building {holder:?} airdrop tx");
        info!(target: "consensus", "[Faucet] ==============================");
        let (airdrop_tx, airdrop_params) = self.airdrop_native(value, holder, None, None)?;

        for h in holders {
            info!(target: "consensus", "[{h:?}] ===============================");
            info!(target: "consensus", "[{h:?}] Executing {holder:?} airdrop tx");
            info!(target: "consensus", "[{h:?}] ===============================");
            self.execute_airdrop_native_tx(h, &airdrop_tx, &airdrop_params, current_slot).await?;
        }

        self.assert_trees(holders);

        // Gather new owncoin
        let oc = self.gather_owncoin(holder, &airdrop_params.outputs[0], None)?;

        Ok(oc)
    }
}
