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
    blockchain::{BlockInfo, Header},
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
use log::info;

use super::{Holder, TestHarness};

impl TestHarness {
    /// Create a `Money::PoWReward` transaction for a given [`Holder`].
    ///
    /// Optionally takes a specific reward recipient and a nonstandard reward value.
    /// Returns the created [`Transaction`] and [`MoneyPoWRewardParamsV1`].
    async fn pow_reward(
        &mut self,
        holder: &Holder,
        recipient: Option<&Holder>,
        reward: Option<u64>,
    ) -> Result<(Transaction, MoneyPoWRewardParamsV1)> {
        let wallet = self.holders.get(holder).unwrap();

        let (mint_pk, mint_zkbin) = self.proving_keys.get(MONEY_CONTRACT_ZKAS_MINT_NS_V1).unwrap();

        // Reference the last block in the holder's blockchain
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
            block_height: last_block.header.height + 1,
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
        let sigs = tx.create_sigs(&[wallet.keypair.secret])?;
        tx.signatures = vec![sigs];

        Ok((tx, debris.params))
    }

    /// Generate and add an empty block to the given [`Holder`]s blockchains.
    /// The `miner` holder will produce the block and receive the reward.
    ///
    /// Returns any found [`OwnCoin`]s.
    pub async fn generate_block(
        &mut self,
        miner: &Holder,
        holders: &[Holder],
    ) -> Result<Vec<OwnCoin>> {
        // Build the POW reward transaction
        info!("Building PoWReward transaction for {:?}", miner);
        let (tx, params) = self.pow_reward(miner, None, None).await?;

        // Fetch the last block in the blockchain
        let wallet = self.holders.get(miner).unwrap();
        let previous = wallet.validator.blockchain.last_block()?;

        // We increment timestamp so we don't have to use sleep
        let timestamp = previous.header.timestamp.checked_add(1.into())?;

        // Generate block header
        let header = Header::new(
            previous.hash()?,
            previous.header.height + 1,
            timestamp,
            previous.header.nonce,
        );

        // Generate the block
        let mut block = BlockInfo::new_empty(header);

        // Add producer transaction to the block
        block.append_txs(vec![tx]);

        // Attach signature
        block.sign(&wallet.keypair.secret)?;

        // For all holders, append the block
        let mut found_owncoins = vec![];
        for holder in holders {
            let wallet = self.holders.get_mut(holder).unwrap();
            wallet.validator.add_blocks(&[block.clone()]).await?;
            wallet.money_merkle_tree.append(MerkleNode::from(params.output.coin.inner()));

            // Attempt to decrypt the note to see if this is a coin for the holder
            let Ok(note) = params.output.note.decrypt::<MoneyNote>(&wallet.keypair.secret) else {
                continue
            };

            let owncoin = OwnCoin {
                coin: params.output.coin,
                note: note.clone(),
                secret: wallet.keypair.secret,
                leaf_position: wallet.money_merkle_tree.mark().unwrap(),
            };

            wallet.unspent_money_coins.push(owncoin.clone());
            found_owncoins.push(owncoin);
        }

        Ok(found_owncoins)
    }
}
