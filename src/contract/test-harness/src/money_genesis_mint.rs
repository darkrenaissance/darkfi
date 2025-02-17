/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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
    client::{genesis_mint_v1::GenesisMintCallBuilder, MoneyNote, OwnCoin},
    model::MoneyGenesisMintParamsV1,
    MoneyFunction, MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};
use darkfi_sdk::{
    crypto::{contract_id::MONEY_CONTRACT_ID, FuncId, MerkleNode},
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::AsyncEncodable;
use log::debug;

use super::{Holder, TestHarness};

impl TestHarness {
    /// Create a `Money::GenesisMint` transaction for a given [`Holder`].
    ///
    /// Returns the created [`Transaction`] and its parameters.
    pub async fn genesis_mint(
        &mut self,
        holder: &Holder,
        amounts: &[u64],
        spend_hook: Option<FuncId>,
        user_data: Option<pallas::Base>,
    ) -> Result<(Transaction, MoneyGenesisMintParamsV1)> {
        let wallet = self.holders.get(holder).unwrap();

        let (mint_pk, mint_zkbin) = self.proving_keys.get(MONEY_CONTRACT_ZKAS_MINT_NS_V1).unwrap();

        // Build the contract call
        let builder = GenesisMintCallBuilder {
            signature_public: wallet.keypair.public,
            amounts: amounts.to_vec(),
            recipient: None,
            spend_hook,
            user_data,
            mint_zkbin: mint_zkbin.clone(),
            mint_pk: mint_pk.clone(),
        };

        let debris = builder.build()?;

        // Encode and build the transaction
        let mut data = vec![MoneyFunction::GenesisMintV1 as u8];
        debris.params.encode_async(&mut data).await?;
        let call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };
        let mut tx_builder =
            TransactionBuilder::new(ContractCallLeaf { call, proofs: debris.proofs }, vec![])?;
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&[wallet.keypair.secret])?;
        tx.signatures = vec![sigs];

        Ok((tx, debris.params))
    }

    /// Execute the [`Transaction`] created by `genesis_mint()`.
    ///
    /// Returns any found [`OwnCoin`]s.
    pub async fn execute_genesis_mint_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        params: &MoneyGenesisMintParamsV1,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.holders.get_mut(holder).unwrap();

        // Execute the transaction
        wallet.add_transaction("money::genesis_mint", tx, block_height).await?;

        if !append {
            return Ok(vec![])
        }

        // Iterate over call outputs to find any new OwnCoins
        let mut found_owncoins = vec![];
        for output in &params.outputs {
            wallet.money_merkle_tree.append(MerkleNode::from(output.coin.inner()));

            // Attempt to decrypt the output note to see if this is a coin for the holder.
            let Ok(note) = output.note.decrypt::<MoneyNote>(&wallet.keypair.secret) else {
                continue
            };

            let owncoin = OwnCoin {
                coin: output.coin,
                note: note.clone(),
                secret: wallet.keypair.secret,
                leaf_position: wallet.money_merkle_tree.mark().unwrap(),
            };

            debug!("Found new OwnCoin({}) for {:?}", owncoin.coin, holder);
            wallet.unspent_money_coins.push(owncoin.clone());
            found_owncoins.push(owncoin);
        }

        Ok(found_owncoins)
    }
}
