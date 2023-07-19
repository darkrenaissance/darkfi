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
    client::{token_freeze_v1::TokenFreezeCallBuilder, token_mint_v1::TokenMintCallBuilder},
    model::{MoneyTokenFreezeParamsV1, MoneyTokenMintParamsV1},
    MoneyFunction, MONEY_CONTRACT_ZKAS_TOKEN_FRZ_NS_V1, MONEY_CONTRACT_ZKAS_TOKEN_MINT_NS_V1,
};
use darkfi_sdk::{
    crypto::{MerkleNode, MONEY_CONTRACT_ID},
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{serialize, Encodable};
use rand::rngs::OsRng;

use super::{Holder, TestHarness, TxAction};

impl TestHarness {
    pub fn token_mint(
        &mut self,
        amount: u64,
        holder: &Holder,
        recipient: &Holder,
        spend_hook: Option<pallas::Base>,
        user_data: Option<pallas::Base>,
    ) -> Result<(Transaction, MoneyTokenMintParamsV1)> {
        let rcpt = self.holders.get(recipient).unwrap().keypair.public;
        let mint_authority = self.holders.get(holder).unwrap().token_mint_authority;
        let (mint_pk, mint_zkbin) =
            self.proving_keys.get(&MONEY_CONTRACT_ZKAS_TOKEN_MINT_NS_V1).unwrap();
        let tx_action_benchmark =
            self.tx_action_benchmarks.get_mut(&TxAction::MoneyTokenMint).unwrap();
        let timer = Instant::now();

        let builder = TokenMintCallBuilder {
            mint_authority,
            recipient: rcpt,
            amount,
            spend_hook: spend_hook.unwrap_or(pallas::Base::ZERO),
            user_data: user_data.unwrap_or(pallas::Base::ZERO),
            token_mint_zkbin: mint_zkbin.clone(),
            token_mint_pk: mint_pk.clone(),
        };

        let debris = builder.build()?;

        let mut data = vec![MoneyFunction::TokenMintV1 as u8];
        debris.params.encode(&mut data)?;
        let calls = vec![ContractCall { contract_id: *MONEY_CONTRACT_ID, data }];
        let proofs = vec![debris.proofs];
        let mut tx = Transaction { calls, proofs, signatures: vec![] };
        let sigs = tx.create_sigs(&mut OsRng, &[mint_authority.secret])?;
        tx.signatures = vec![sigs];
        tx_action_benchmark.creation_times.push(timer.elapsed());

        // Calculate transaction sizes
        let encoded: Vec<u8> = serialize(&tx);
        let size = std::mem::size_of_val(&*encoded);
        tx_action_benchmark.sizes.push(size);
        let base58 = bs58::encode(&encoded).into_string();
        let size = std::mem::size_of_val(&*base58);
        tx_action_benchmark.broadcasted_sizes.push(size);

        Ok((tx, debris.params))
    }

    pub async fn execute_token_mint_tx(
        &mut self,
        holder: &Holder,
        tx: &Transaction,
        params: &MoneyTokenMintParamsV1,
        slot: u64,
    ) -> Result<()> {
        let wallet = self.holders.get_mut(holder).unwrap();
        let tx_action_benchmark =
            self.tx_action_benchmarks.get_mut(&TxAction::MoneyTokenMint).unwrap();
        let timer = Instant::now();

        wallet.validator.read().await.add_transactions(&[tx.clone()], slot, true).await?;
        wallet.money_merkle_tree.append(MerkleNode::from(params.output.coin.inner()));
        tx_action_benchmark.verify_times.push(timer.elapsed());

        Ok(())
    }

    pub fn token_freeze(
        &mut self,
        holder: &Holder,
    ) -> Result<(Transaction, MoneyTokenFreezeParamsV1)> {
        let mint_authority = self.holders.get(holder).unwrap().token_mint_authority;
        let (frz_pk, frz_zkbin) =
            self.proving_keys.get(&MONEY_CONTRACT_ZKAS_TOKEN_FRZ_NS_V1).unwrap();
        let tx_action_benchmark =
            self.tx_action_benchmarks.get_mut(&TxAction::MoneyTokenFreeze).unwrap();
        let timer = Instant::now();

        let builder = TokenFreezeCallBuilder {
            mint_authority,
            token_freeze_zkbin: frz_zkbin.clone(),
            token_freeze_pk: frz_pk.clone(),
        };

        let debris = builder.build()?;

        let mut data = vec![MoneyFunction::TokenFreezeV1 as u8];
        debris.params.encode(&mut data)?;
        let calls = vec![ContractCall { contract_id: *MONEY_CONTRACT_ID, data }];
        let proofs = vec![debris.proofs];
        let mut tx = Transaction { calls, proofs, signatures: vec![] };
        let sigs = tx.create_sigs(&mut OsRng, &[mint_authority.secret])?;
        tx.signatures = vec![sigs];
        tx_action_benchmark.creation_times.push(timer.elapsed());

        // Calculate transaction sizes
        let encoded: Vec<u8> = serialize(&tx);
        let size = std::mem::size_of_val(&*encoded);
        tx_action_benchmark.sizes.push(size);
        let base58 = bs58::encode(&encoded).into_string();
        let size = std::mem::size_of_val(&*base58);
        tx_action_benchmark.broadcasted_sizes.push(size);

        Ok((tx, debris.params))
    }

    pub async fn execute_token_freeze_tx(
        &mut self,
        holder: &Holder,
        tx: &Transaction,
        _params: &MoneyTokenFreezeParamsV1,
        slot: u64,
    ) -> Result<()> {
        let wallet = self.holders.get_mut(holder).unwrap();
        let tx_action_benchmark =
            self.tx_action_benchmarks.get_mut(&TxAction::MoneyTokenFreeze).unwrap();
        let timer = Instant::now();

        wallet.validator.read().await.add_transactions(&[tx.clone()], slot, true).await?;
        tx_action_benchmark.verify_times.push(timer.elapsed());

        Ok(())
    }
}
