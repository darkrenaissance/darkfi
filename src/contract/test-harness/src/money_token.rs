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

use std::time::Instant;

use darkfi::{
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    zk::halo2::Field,
    Result,
};
use darkfi_money_contract::{
    client::{
        auth_token_mint_v1::AuthTokenMintCallBuilder, token_freeze_v1::TokenFreezeCallBuilder,
        token_mint_v1::TokenMintCallBuilder,
    },
    model::{
        CoinAttributes, MoneyAuthTokenMintParamsV1, MoneyTokenFreezeParamsV1,
        MoneyTokenMintParamsV1, TokenAttributes,
    },
    MoneyFunction, MONEY_CONTRACT_ZKAS_AUTH_TOKEN_MINT_NS_V1, MONEY_CONTRACT_ZKAS_TOKEN_FRZ_NS_V1,
    MONEY_CONTRACT_ZKAS_TOKEN_MINT_NS_V1,
};
use darkfi_sdk::{
    crypto::{poseidon_hash, Blind, FuncId, FuncRef, MerkleNode, MONEY_CONTRACT_ID},
    dark_tree::DarkLeaf,
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
        spend_hook: Option<FuncId>,
        user_data: Option<pallas::Base>,
    ) -> Result<(Transaction, MoneyTokenMintParamsV1, MoneyAuthTokenMintParamsV1)> {
        let wallet = self.holders.get(holder).unwrap();
        let mint_authority = wallet.token_mint_authority;
        let token_blind = wallet.token_blind;

        let rcpt = self.holders.get(recipient).unwrap().keypair.public;

        let (mint_pk, mint_zkbin) = self
            .proving_keys
            .get(&MONEY_CONTRACT_ZKAS_TOKEN_MINT_NS_V1.to_string())
            .unwrap()
            .clone();
        let (auth_mint_pk, auth_mint_zkbin) = self
            .proving_keys
            .get(&MONEY_CONTRACT_ZKAS_AUTH_TOKEN_MINT_NS_V1.to_string())
            .unwrap()
            .clone();

        let tx_action_benchmark =
            self.tx_action_benchmarks.get_mut(&TxAction::MoneyTokenMint).unwrap();

        let timer = Instant::now();

        let auth_func_id = FuncRef {
            contract_id: *MONEY_CONTRACT_ID,
            func_code: MoneyFunction::AuthTokenMintV1 as u8,
        }
        .to_func_id();

        let token_attrs = TokenAttributes {
            auth_parent: auth_func_id,
            user_data: poseidon_hash([mint_authority.public.x(), mint_authority.public.y()]),
            blind: token_blind,
        };
        let token_id = token_attrs.to_token_id();

        let coin_attrs = CoinAttributes {
            public_key: rcpt,
            value: amount,
            token_id,
            spend_hook: spend_hook.unwrap_or(FuncId::none()),
            user_data: user_data.unwrap_or(pallas::Base::ZERO),
            blind: Blind::random(&mut OsRng),
        };

        let builder = TokenMintCallBuilder {
            coin_attrs: coin_attrs.clone(),
            token_attrs: token_attrs.clone(),
            mint_zkbin,
            mint_pk,
        };
        let mint_debris = builder.build()?;
        let mut data = vec![MoneyFunction::TokenMintV1 as u8];
        mint_debris.params.encode(&mut data)?;
        let mint_call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };

        let builder = AuthTokenMintCallBuilder {
            coin_attrs,
            token_attrs,
            mint_keypair: mint_authority,
            auth_mint_zkbin,
            auth_mint_pk,
        };
        let auth_debris = builder.build()?;
        let mut data = vec![MoneyFunction::AuthTokenMintV1 as u8];
        auth_debris.params.encode(&mut data)?;
        let auth_call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };

        let mut tx = Transaction {
            calls: vec![
                DarkLeaf { data: mint_call, parent_index: Some(1), children_indexes: vec![] },
                DarkLeaf { data: auth_call, parent_index: None, children_indexes: vec![0] },
            ],
            proofs: vec![mint_debris.proofs, auth_debris.proofs],
            signatures: vec![],
        };
        let mint_sigs = tx.create_sigs(&mut OsRng, &[])?;
        let auth_sigs = tx.create_sigs(&mut OsRng, &[mint_authority.secret])?;
        tx.signatures = vec![mint_sigs, auth_sigs];
        tx_action_benchmark.creation_times.push(timer.elapsed());

        // Calculate transaction sizes
        let encoded: Vec<u8> = serialize(&tx);
        let size = std::mem::size_of_val(&*encoded);
        tx_action_benchmark.sizes.push(size);
        let base58 = bs58::encode(&encoded).into_string();
        let size = std::mem::size_of_val(&*base58);
        tx_action_benchmark.broadcasted_sizes.push(size);

        Ok((tx, mint_debris.params, auth_debris.params))
    }

    pub async fn execute_token_mint_tx(
        &mut self,
        holder: &Holder,
        tx: &Transaction,
        params: &MoneyTokenMintParamsV1,
        block_height: u64,
    ) -> Result<()> {
        let wallet = self.holders.get_mut(holder).unwrap();
        let tx_action_benchmark =
            self.tx_action_benchmarks.get_mut(&TxAction::MoneyTokenMint).unwrap();
        let timer = Instant::now();

        wallet.validator.add_transactions(&[tx.clone()], block_height, true).await?;
        wallet.money_merkle_tree.append(MerkleNode::from(params.coin.inner()));

        tx_action_benchmark.verify_times.push(timer.elapsed());

        Ok(())
    }

    pub fn token_freeze(
        &mut self,
        holder: &Holder,
    ) -> Result<(Transaction, MoneyTokenFreezeParamsV1)> {
        let wallet = self.holders.get(holder).unwrap();
        let mint_keypair = wallet.token_mint_authority;
        let token_blind = wallet.token_blind;

        let (frz_pk, frz_zkbin) =
            self.proving_keys.get(&MONEY_CONTRACT_ZKAS_TOKEN_FRZ_NS_V1.to_string()).unwrap();

        let tx_action_benchmark =
            self.tx_action_benchmarks.get_mut(&TxAction::MoneyTokenFreeze).unwrap();

        let timer = Instant::now();

        let auth_func_id = FuncRef {
            contract_id: *MONEY_CONTRACT_ID,
            func_code: MoneyFunction::AuthTokenMintV1 as u8,
        }
        .to_func_id();

        let token_attrs = TokenAttributes {
            auth_parent: auth_func_id,
            user_data: poseidon_hash([mint_keypair.public.x(), mint_keypair.public.y()]),
            blind: token_blind,
        };

        let builder = TokenFreezeCallBuilder {
            mint_keypair,
            token_attrs,
            freeze_zkbin: frz_zkbin.clone(),
            freeze_pk: frz_pk.clone(),
        };

        let debris = builder.build()?;

        let mut data = vec![MoneyFunction::TokenFreezeV1 as u8];
        debris.params.encode(&mut data)?;
        let call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };
        let mut tx_builder =
            TransactionBuilder::new(ContractCallLeaf { call, proofs: debris.proofs }, vec![])?;
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&mut OsRng, &[mint_keypair.secret])?;
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
        block_height: u64,
    ) -> Result<()> {
        let wallet = self.holders.get_mut(holder).unwrap();
        let tx_action_benchmark =
            self.tx_action_benchmarks.get_mut(&TxAction::MoneyTokenFreeze).unwrap();
        let timer = Instant::now();

        wallet.validator.add_transactions(&[tx.clone()], block_height, true).await?;
        tx_action_benchmark.verify_times.push(timer.elapsed());

        Ok(())
    }
}
