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
    client::{
        auth_token_mint_v1::AuthTokenMintCallBuilder, token_freeze_v1::TokenFreezeCallBuilder,
        token_mint_v1::TokenMintCallBuilder, MoneyNote, OwnCoin,
    },
    model::{
        CoinAttributes, MoneyAuthTokenMintParamsV1, MoneyFeeParamsV1, MoneyTokenFreezeParamsV1,
        MoneyTokenMintParamsV1, TokenAttributes,
    },
    MoneyFunction, MONEY_CONTRACT_ZKAS_AUTH_TOKEN_MINT_NS_V1, MONEY_CONTRACT_ZKAS_TOKEN_FRZ_NS_V1,
    MONEY_CONTRACT_ZKAS_TOKEN_MINT_NS_V1,
};
use darkfi_sdk::{
    crypto::{poseidon_hash, BaseBlind, Blind, FuncId, FuncRef, MerkleNode, MONEY_CONTRACT_ID},
    dark_tree::DarkTree,
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::AsyncEncodable;
use log::debug;
use rand::rngs::OsRng;

use super::{Holder, TestHarness};

impl TestHarness {
    /// Mint an arbitrary token for a given recipient using `Money::TokenMint`
    #[allow(clippy::too_many_arguments)]
    pub async fn token_mint(
        &mut self,
        amount: u64,
        holder: &Holder,
        recipient: &Holder,
        token_blind: BaseBlind,
        spend_hook: Option<FuncId>,
        user_data: Option<pallas::Base>,
        block_height: u64,
    ) -> Result<(
        Transaction,
        MoneyTokenMintParamsV1,
        MoneyAuthTokenMintParamsV1,
        Option<MoneyFeeParamsV1>,
    )> {
        let wallet = self.holders.get(holder).unwrap();
        let mint_authority = wallet.token_mint_authority;
        let rcpt = self.holders.get(recipient).unwrap().keypair.public;

        let (token_mint_pk, token_mint_zkbin) =
            self.proving_keys.get(&MONEY_CONTRACT_ZKAS_TOKEN_MINT_NS_V1.to_string()).unwrap();

        let (auth_mint_pk, auth_mint_zkbin) =
            self.proving_keys.get(&MONEY_CONTRACT_ZKAS_AUTH_TOKEN_MINT_NS_V1.to_string()).unwrap();

        // Create the Auth FuncID
        let auth_func_id = FuncRef {
            contract_id: *MONEY_CONTRACT_ID,
            func_code: MoneyFunction::AuthTokenMintV1 as u8,
        }
        .to_func_id();

        let (mint_auth_x, mint_auth_y) = mint_authority.public.xy();

        let token_attrs = TokenAttributes {
            auth_parent: auth_func_id,
            user_data: poseidon_hash([mint_auth_x, mint_auth_y]),
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

        // Create the minting call
        let builder = TokenMintCallBuilder {
            coin_attrs: coin_attrs.clone(),
            token_attrs: token_attrs.clone(),
            mint_zkbin: token_mint_zkbin.clone(),
            mint_pk: token_mint_pk.clone(),
        };
        let mint_debris = builder.build()?;
        let mut data = vec![MoneyFunction::TokenMintV1 as u8];
        mint_debris.params.encode_async(&mut data).await?;
        let mint_call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };

        // Create the auth call
        let builder = AuthTokenMintCallBuilder {
            coin_attrs,
            token_attrs,
            mint_keypair: mint_authority,
            auth_mint_zkbin: auth_mint_zkbin.clone(),
            auth_mint_pk: auth_mint_pk.clone(),
        };
        let auth_debris = builder.build()?;
        let mut data = vec![MoneyFunction::AuthTokenMintV1 as u8];
        auth_debris.params.encode_async(&mut data).await?;
        let auth_call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };

        // Create the TransactionBuilder containing above calls
        let mut tx_builder = TransactionBuilder::new(
            ContractCallLeaf { call: auth_call, proofs: auth_debris.proofs },
            vec![DarkTree::new(
                ContractCallLeaf { call: mint_call, proofs: mint_debris.proofs },
                vec![],
                None,
                None,
            )],
        )?;

        // If we have tx fees enabled, make an offering
        let mut fee_params = None;
        let mut fee_signature_secrets = None;
        if self.verify_fees {
            let mut tx = tx_builder.build()?;
            let mint_sigs = tx.create_sigs(&[])?;
            let auth_sigs = tx.create_sigs(&[mint_authority.secret])?;
            tx.signatures = vec![mint_sigs, auth_sigs];

            let (fee_call, fee_proofs, fee_secrets, _spent_fee_coins, fee_call_params) =
                self.append_fee_call(holder, tx, block_height, &[]).await?;

            // Append the fee call to the transaction
            tx_builder.append(ContractCallLeaf { call: fee_call, proofs: fee_proofs }, vec![])?;
            fee_signature_secrets = Some(fee_secrets);
            fee_params = Some(fee_call_params);
        }

        // Now build the actual transaction and sign it with necessary keys.
        let mut tx = tx_builder.build()?;
        let mint_sigs = tx.create_sigs(&[])?;
        let auth_sigs = tx.create_sigs(&[mint_authority.secret])?;
        tx.signatures = vec![mint_sigs, auth_sigs];
        if let Some(fee_signature_secrets) = fee_signature_secrets {
            let sigs = tx.create_sigs(&fee_signature_secrets)?;
            tx.signatures.push(sigs);
        }

        Ok((tx, mint_debris.params, auth_debris.params, fee_params))
    }

    /// Execute the transaction created by `token_mint()` for a given [`Holder`].
    ///
    /// Returns any found [`OwnCoin`]s.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute_token_mint_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        mint_params: &MoneyTokenMintParamsV1,
        auth_params: &MoneyAuthTokenMintParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u64,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.holders.get_mut(holder).unwrap();

        // Execute the transaction
        wallet.validator.add_transactions(&[tx], block_height, true, self.verify_fees).await?;

        // Iterate over all inputs to mark any spent coins
        if let Some(ref fee_params) = fee_params {
            if append {
                if let Some(spent_coin) = wallet
                    .unspent_money_coins
                    .iter()
                    .find(|x| x.nullifier() == fee_params.input.nullifier)
                    .cloned()
                {
                    debug!("Found spent OwnCoin({}) for {:?}", spent_coin.coin, holder);
                    wallet
                        .unspent_money_coins
                        .retain(|x| x.nullifier() != fee_params.input.nullifier);
                    wallet.spent_money_coins.push(spent_coin.clone());
                }
            }
        }

        let mut found_owncoins = vec![];

        if append {
            wallet.money_merkle_tree.append(MerkleNode::from(mint_params.coin.inner()));

            // Attempt to decrypt the encrypted note of the minted token
            if let Ok(note) = auth_params.enc_note.decrypt::<MoneyNote>(&wallet.keypair.secret) {
                let owncoin = OwnCoin {
                    coin: mint_params.coin,
                    note: note.clone(),
                    secret: wallet.keypair.secret,
                    leaf_position: wallet.money_merkle_tree.mark().unwrap(),
                };

                debug!("Found new OwnCoin({}) for {:?}", owncoin.coin, holder);
                wallet.unspent_money_coins.push(owncoin.clone());
                found_owncoins.push(owncoin);
            };

            if let Some(ref fee_params) = fee_params {
                wallet.money_merkle_tree.append(MerkleNode::from(fee_params.output.coin.inner()));

                // Attempt to decrypt the encrypted note in the fee output
                if let Ok(note) =
                    fee_params.output.note.decrypt::<MoneyNote>(&wallet.keypair.secret)
                {
                    let owncoin = OwnCoin {
                        coin: fee_params.output.coin,
                        note: note.clone(),
                        secret: wallet.keypair.secret,
                        leaf_position: wallet.money_merkle_tree.mark().unwrap(),
                    };

                    debug!("Found new OwnCoin({}) for {:?}", owncoin.coin, holder);
                    wallet.unspent_money_coins.push(owncoin.clone());
                    found_owncoins.push(owncoin);
                }
            }
        }

        Ok(found_owncoins)
    }

    /// Freeze the supply of a minted token
    pub async fn token_freeze(
        &mut self,
        holder: &Holder,
        block_height: u64,
    ) -> Result<(Transaction, MoneyTokenFreezeParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.holders.get(holder).unwrap();
        let mint_authority = wallet.token_mint_authority;

        let (frz_pk, frz_zkbin) =
            self.proving_keys.get(&MONEY_CONTRACT_ZKAS_TOKEN_FRZ_NS_V1.to_string()).unwrap();

        let auth_func_id = FuncRef {
            contract_id: *MONEY_CONTRACT_ID,
            func_code: MoneyFunction::AuthTokenMintV1 as u8,
        }
        .to_func_id();

        let (mint_auth_x, mint_auth_y) = mint_authority.public.xy();
        let token_blind = BaseBlind::random(&mut OsRng);

        let token_attrs = TokenAttributes {
            auth_parent: auth_func_id,
            user_data: poseidon_hash([mint_auth_x, mint_auth_y]),
            blind: token_blind,
        };

        // Create the freeze call
        let builder = TokenFreezeCallBuilder {
            mint_keypair: mint_authority,
            token_attrs,
            freeze_zkbin: frz_zkbin.clone(),
            freeze_pk: frz_pk.clone(),
        };
        let freeze_debris = builder.build()?;
        let mut data = vec![MoneyFunction::TokenFreezeV1 as u8];
        freeze_debris.params.encode_async(&mut data).await?;
        let freeze_call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };

        // Create the TransactionBuilder containing the above call
        let mut tx_builder = TransactionBuilder::new(
            ContractCallLeaf { call: freeze_call, proofs: freeze_debris.proofs },
            vec![],
        )?;

        // If we have tx fees enabled, make an offering
        let mut fee_params = None;
        let mut fee_signature_secrets = None;
        if self.verify_fees {
            let mut tx = tx_builder.build()?;
            let freeze_sigs = tx.create_sigs(&[mint_authority.secret])?;
            tx.signatures = vec![freeze_sigs];

            let (fee_call, fee_proofs, fee_secrets, _spent_fee_coins, fee_call_params) =
                self.append_fee_call(holder, tx, block_height, &[]).await?;

            // Append the fee call to the transaction
            tx_builder.append(ContractCallLeaf { call: fee_call, proofs: fee_proofs }, vec![])?;
            fee_signature_secrets = Some(fee_secrets);
            fee_params = Some(fee_call_params);
        }

        // Now build the actual transaction and sign it with necessary keys.
        let mut tx = tx_builder.build()?;
        let freeze_sigs = tx.create_sigs(&[mint_authority.secret])?;
        tx.signatures = vec![freeze_sigs];
        if let Some(fee_signature_secrets) = fee_signature_secrets {
            let sigs = tx.create_sigs(&fee_signature_secrets)?;
            tx.signatures.push(sigs);
        }

        Ok((tx, freeze_debris.params, fee_params))
    }

    /// Execute the transaction created by `token_freeze()` for a given [`Holder`].
    ///
    /// Returns any found [`OwnCoin`]s.
    pub async fn execute_token_freeze_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _freeze_params: &MoneyTokenFreezeParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u64,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.holders.get_mut(holder).unwrap();

        // Execute the transaction
        wallet.validator.add_transactions(&[tx], block_height, true, self.verify_fees).await?;

        let mut found_owncoins = vec![];
        if let Some(ref fee_params) = fee_params {
            if append {
                if let Some(spent_coin) = wallet
                    .unspent_money_coins
                    .iter()
                    .find(|x| x.nullifier() == fee_params.input.nullifier)
                    .cloned()
                {
                    debug!("Found spent OwnCoin({}) for {:?}", spent_coin.coin, holder);
                    wallet
                        .unspent_money_coins
                        .retain(|x| x.nullifier() != fee_params.input.nullifier);
                    wallet.spent_money_coins.push(spent_coin.clone());
                }

                wallet.money_merkle_tree.append(MerkleNode::from(fee_params.output.coin.inner()));

                // Attempt to decrypt the encrypted note
                if let Ok(note) =
                    fee_params.output.note.decrypt::<MoneyNote>(&wallet.keypair.secret)
                {
                    let owncoin = OwnCoin {
                        coin: fee_params.output.coin,
                        note: note.clone(),
                        secret: wallet.keypair.secret,
                        leaf_position: wallet.money_merkle_tree.mark().unwrap(),
                    };

                    debug!("Found new OwnCoin({}) for {:?}", owncoin.coin, holder);
                    wallet.unspent_money_coins.push(owncoin.clone());
                    found_owncoins.push(owncoin);
                }
            }
        }

        Ok(found_owncoins)
    }
}
