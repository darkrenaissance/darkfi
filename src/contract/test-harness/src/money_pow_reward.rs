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
    zk::halo2::Field,
    Result,
};
use darkfi_money_contract::{
    client::{pow_reward_v1::PoWRewardCallBuilder, MoneyNote, OwnCoin},
    model::MoneyPoWRewardParamsV1,
    MoneyFunction, MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};
use darkfi_sdk::{
    crypto::{contract_id::MONEY_CONTRACT_ID, FuncId, MerkleNode},
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::AsyncEncodable;
use rand::rngs::OsRng;

use super::{Holder, TestHarness};

impl TestHarness {
    /// Create a `Money::PoWReward` transaction for a given [`Holder`].
    ///
    /// Optionally takes a specific reward recipient and a nonstandard reward value.
    /// Returns the created [`Transaction`] and [`MoneyPoWRewardParamsV1`].
    pub async fn pow_reward(
        &mut self,
        holder: &Holder,
        recipient: Option<&Holder>,
        reward: Option<u64>,
    ) -> Result<(Transaction, MoneyPoWRewardParamsV1)> {
        let wallet = self.holders.get(holder).unwrap();

        let (mint_pk, mint_zkbin) =
            self.proving_keys.get(&MONEY_CONTRACT_ZKAS_MINT_NS_V1.to_string()).unwrap();

        // Reference the last block in the holder's blockchain
        let (block_height, fork_previous_hash) = wallet.validator.blockchain.last()?;
        let last_block = wallet.validator.blockchain.last_block()?;

        // If there's a set reward recipient, use it, otherwise reward the holder
        let recipient = if let Some(holder) = recipient {
            self.holders.get(holder).unwrap().keypair.public
        } else {
            wallet.keypair.public
        };

        // Build the transaction
        let builder = PoWRewardCallBuilder {
            secret: wallet.keypair.secret,
            recipient,
            block_height: block_height + 1,
            last_nonce: last_block.header.nonce,
            fork_previous_hash,
            spend_hook: FuncId::none(),
            user_data: pallas::Base::ZERO,
            mint_zkbin: mint_zkbin.clone(),
            mint_pk: mint_pk.clone(),
        };

        let debris = match reward {
            Some(value) => builder.build_with_custom_reward(value)?,
            None => builder.build()?,
        };

        // Encode the transaction
        let mut data = vec![MoneyFunction::PoWRewardV1 as u8];
        debris.params.encode_async(&mut data).await?;
        let call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };
        let mut tx_builder =
            TransactionBuilder::new(ContractCallLeaf { call, proofs: debris.proofs }, vec![])?;
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&mut OsRng, &[wallet.keypair.secret])?;
        tx.signatures = vec![sigs];

        Ok((tx, debris.params))
    }

    /// Execute the transaction created by `pow_reward()` for a given [`Holder`].
    ///
    /// Returns any gathered [`OwnCoin`]s from the transaction.
    pub async fn execute_pow_reward_tx(
        &mut self,
        holder: &Holder,
        tx: &Transaction,
        params: &MoneyPoWRewardParamsV1,
        block_height: u64,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.holders.get_mut(holder).unwrap();

        wallet.validator.add_test_producer_transaction(tx, block_height, true).await?;
        wallet.money_merkle_tree.append(MerkleNode::from(params.output.coin.inner()));

        // Attempt to decrypt the output note to see if this is a coin for the holder.
        let Ok(note) = params.output.note.decrypt::<MoneyNote>(&wallet.keypair.secret) else {
            return Ok(vec![])
        };

        let owncoin = OwnCoin {
            coin: params.output.coin,
            note: note.clone(),
            secret: wallet.keypair.secret,
            leaf_position: wallet.money_merkle_tree.mark().unwrap(),
        };

        wallet.unspent_money_coins.push(owncoin.clone());
        Ok(vec![owncoin])
    }
}
