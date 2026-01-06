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

use rand::rngs::OsRng;
use rusqlite::types::Value;

use darkfi::{
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    util::parse::decode_base10,
    zk::{halo2::Field, proof::ProvingKey, vm::ZkCircuit, vm_heap::empty_witnesses},
    zkas::ZkBinary,
    Error, Result,
};
use darkfi_money_contract::{
    client::{
        auth_token_freeze_v1::AuthTokenFreezeCallBuilder,
        auth_token_mint_v1::AuthTokenMintCallBuilder, token_mint_v1::TokenMintCallBuilder,
    },
    model::{CoinAttributes, TokenAttributes, TokenId},
    MoneyFunction, MONEY_CONTRACT_ZKAS_AUTH_TOKEN_MINT_NS_V1, MONEY_CONTRACT_ZKAS_FEE_NS_V1,
    MONEY_CONTRACT_ZKAS_TOKEN_MINT_NS_V1,
};
use darkfi_sdk::{
    crypto::{
        contract_id::MONEY_CONTRACT_ID, poseidon_hash, BaseBlind, Blind, FuncId, FuncRef, Keypair,
        PublicKey, SecretKey,
    },
    dark_tree::DarkTree,
    pasta::pallas,
    tx::ContractCall,
};
use darkfi_serial::{deserialize_async, serialize_async, AsyncEncodable};

use crate::{
    convert_named_params,
    error::WalletDbResult,
    money::{
        BALANCE_BASE10_DECIMALS, MONEY_TOKENS_COL_FREEZE_HEIGHT, MONEY_TOKENS_COL_IS_FROZEN,
        MONEY_TOKENS_COL_MINT_AUTHORITY, MONEY_TOKENS_COL_TOKEN_BLIND, MONEY_TOKENS_COL_TOKEN_ID,
        MONEY_TOKENS_TABLE,
    },
    Drk,
};

impl Drk {
    /// Auxiliary function to derive `TokenAttributes` for provided secret key and token blind.
    fn derive_token_attributes(
        &self,
        mint_authority: SecretKey,
        token_blind: BaseBlind,
    ) -> TokenAttributes {
        // Create the Auth FuncID
        let auth_func_id = FuncRef {
            contract_id: *MONEY_CONTRACT_ID,
            func_code: MoneyFunction::AuthTokenMintV1 as u8,
        }
        .to_func_id();

        // Grab the mint authority key public coordinates
        let (mint_auth_x, mint_auth_y) = PublicKey::from_secret(mint_authority).xy();

        // Generate the token attributes
        TokenAttributes {
            auth_parent: auth_func_id,
            user_data: poseidon_hash([mint_auth_x, mint_auth_y]),
            blind: token_blind,
        }
    }

    /// Import a token mint authority into the wallet.
    pub async fn import_mint_authority(
        &self,
        mint_authority: SecretKey,
        token_blind: BaseBlind,
    ) -> Result<TokenId> {
        let token_id = self.derive_token_attributes(mint_authority, token_blind).to_token_id();
        let is_frozen = 0;
        let freeze_height: Option<u32> = None;

        let query = format!(
            "INSERT INTO {} ({}, {}, {}, {}, {}) VALUES (?1, ?2, ?3, ?4, ?5);",
            *MONEY_TOKENS_TABLE,
            MONEY_TOKENS_COL_TOKEN_ID,
            MONEY_TOKENS_COL_MINT_AUTHORITY,
            MONEY_TOKENS_COL_TOKEN_BLIND,
            MONEY_TOKENS_COL_IS_FROZEN,
            MONEY_TOKENS_COL_FREEZE_HEIGHT,
        );

        if let Err(e) = self.wallet.exec_sql(
            &query,
            rusqlite::params![
                serialize_async(&token_id).await,
                serialize_async(&mint_authority).await,
                serialize_async(&token_blind).await,
                is_frozen,
                freeze_height,
            ],
        ) {
            return Err(Error::DatabaseError(format!(
                "[import_mint_authority] Inserting mint authority failed: {e}"
            )))
        };

        Ok(token_id)
    }

    /// Auxiliary function to parse a `MONEY_TOKENS_TABLE` records.
    /// The boolean in the returned tuples notes if the token mint authority is frozen.
    async fn parse_mint_authority_record(
        &self,
        row: &[Value],
    ) -> Result<(TokenId, SecretKey, BaseBlind, bool, Option<u32>)> {
        let Value::Blob(ref token_bytes) = row[0] else {
            return Err(Error::ParseFailed(
                "[parse_mint_authority_record] Token ID bytes parsing failed",
            ))
        };
        let token_id = deserialize_async(token_bytes).await?;

        let Value::Blob(ref auth_bytes) = row[1] else {
            return Err(Error::ParseFailed(
                "[parse_mint_authority_record] Mint authority bytes parsing failed",
            ))
        };
        let mint_authority = deserialize_async(auth_bytes).await?;

        let Value::Blob(ref token_blind_bytes) = row[2] else {
            return Err(Error::ParseFailed(
                "[parse_mint_authority_record] Token blind bytes parsing failed",
            ))
        };
        let token_blind: BaseBlind = deserialize_async(token_blind_bytes).await?;

        let Value::Integer(frozen) = row[3] else {
            return Err(Error::ParseFailed("[parse_mint_authority_record] Is frozen parsing failed"))
        };
        let Ok(frozen) = i32::try_from(frozen) else {
            return Err(Error::ParseFailed("[parse_mint_authority_record] Is frozen parsing failed"))
        };

        let freeze_height = match row[4] {
            Value::Integer(freeze_height) => {
                let Ok(freeze_height) = u32::try_from(freeze_height) else {
                    return Err(Error::ParseFailed(
                        "[parse_mint_authority_record] Freeze height parsing failed",
                    ))
                };
                Some(freeze_height)
            }
            Value::Null => None,
            _ => {
                return Err(Error::ParseFailed(
                    "[parse_mint_authority_record] Freeze height parsing failed",
                ))
            }
        };

        Ok((token_id, mint_authority, token_blind, frozen != 0, freeze_height))
    }

    /// Reset all token mint authorities frozen status in the wallet.
    pub fn reset_mint_authorities(&self, output: &mut Vec<String>) -> WalletDbResult<()> {
        output.push(String::from("Resetting mint authorities frozen status"));
        let query = format!(
            "UPDATE {} SET {} = 0, {} = NULL;",
            *MONEY_TOKENS_TABLE, MONEY_TOKENS_COL_IS_FROZEN, MONEY_TOKENS_COL_FREEZE_HEIGHT
        );
        self.wallet.exec_sql(&query, &[])?;
        output.push(String::from("Successfully reset mint authorities frozen status"));

        Ok(())
    }

    /// Remove token mint authorities frozen status in the wallet that
    /// where frozen after provided height.
    pub fn unfreeze_mint_authorities_after(
        &self,
        height: &u32,
        output: &mut Vec<String>,
    ) -> WalletDbResult<()> {
        output.push(format!("Resetting mint authorities frozen status after: {height}"));
        let query = format!(
            "UPDATE {} SET {} = 0, {} = NULL WHERE {} > ?1;",
            *MONEY_TOKENS_TABLE,
            MONEY_TOKENS_COL_IS_FROZEN,
            MONEY_TOKENS_COL_FREEZE_HEIGHT,
            MONEY_TOKENS_COL_FREEZE_HEIGHT
        );
        self.wallet.exec_sql(&query, rusqlite::params![Some(*height)])?;
        output.push(String::from("Successfully reset mint authorities frozen status"));

        Ok(())
    }

    /// Fetch all token mint authorities from the wallet.
    pub async fn get_mint_authorities(
        &self,
    ) -> Result<Vec<(TokenId, SecretKey, BaseBlind, bool, Option<u32>)>> {
        let rows = match self.wallet.query_multiple(&MONEY_TOKENS_TABLE, &[], &[]) {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::DatabaseError(format!(
                    "[get_mint_authorities] Tokens mint autorities retrieval failed: {e}"
                )))
            }
        };

        let mut ret = Vec::with_capacity(rows.len());
        for row in rows {
            ret.push(self.parse_mint_authority_record(&row).await?);
        }

        Ok(ret)
    }

    /// Fetch provided token unfrozen mint authority from the wallet.
    async fn get_token_mint_authority(
        &self,
        token_id: &TokenId,
    ) -> Result<(TokenId, SecretKey, BaseBlind, bool, Option<u32>)> {
        let row = match self.wallet.query_single(
            &MONEY_TOKENS_TABLE,
            &[],
            convert_named_params! {(MONEY_TOKENS_COL_TOKEN_ID, serialize_async(token_id).await)},
        ) {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::DatabaseError(format!(
                    "[get_token_mint_authority] Token mint authority retrieval failed: {e}"
                )))
            }
        };

        let token = self.parse_mint_authority_record(&row).await?;

        if token.3 {
            return Err(Error::Custom(
                "This token mint is marked as frozen in the wallet".to_string(),
            ))
        }

        Ok(token)
    }

    /// Create a token mint transaction. Returns the transaction object on success.
    pub async fn mint_token(
        &self,
        amount: &str,
        recipient: PublicKey,
        token_id: TokenId,
        spend_hook: Option<FuncId>,
        user_data: Option<pallas::Base>,
    ) -> Result<Transaction> {
        // Decode provided amount
        let amount = decode_base10(amount, BALANCE_BASE10_DECIMALS, false)?;

        // Grab token ID mint authority and attributes
        let token_mint_authority = self.get_token_mint_authority(&token_id).await?;
        let token_attrs =
            self.derive_token_attributes(token_mint_authority.1, token_mint_authority.2);
        let mint_authority = Keypair::new(token_mint_authority.1);

        // Sanity check
        assert_eq!(token_id, token_attrs.to_token_id());

        // Now we need to do a lookup for the zkas proof bincodes, and create
        // the circuit objects and proving keys so we can build the transaction.
        // We also do this through the RPC.
        let zkas_bins = self.lookup_zkas(&MONEY_CONTRACT_ID).await?;

        let Some(mint_zkbin) =
            zkas_bins.iter().find(|x| x.0 == MONEY_CONTRACT_ZKAS_TOKEN_MINT_NS_V1)
        else {
            return Err(Error::Custom("Token mint circuit not found".to_string()))
        };

        let Some(auth_mint_zkbin) =
            zkas_bins.iter().find(|x| x.0 == MONEY_CONTRACT_ZKAS_AUTH_TOKEN_MINT_NS_V1)
        else {
            return Err(Error::Custom("Auth token mint circuit not found".to_string()))
        };

        let Some(fee_zkbin) = zkas_bins.iter().find(|x| x.0 == MONEY_CONTRACT_ZKAS_FEE_NS_V1)
        else {
            return Err(Error::Custom("Fee circuit not found".to_string()))
        };

        let mint_zkbin = ZkBinary::decode(&mint_zkbin.1, false)?;
        let auth_mint_zkbin = ZkBinary::decode(&auth_mint_zkbin.1, false)?;
        let fee_zkbin = ZkBinary::decode(&fee_zkbin.1, false)?;

        let mint_circuit = ZkCircuit::new(empty_witnesses(&mint_zkbin)?, &mint_zkbin);
        let auth_mint_circuit =
            ZkCircuit::new(empty_witnesses(&auth_mint_zkbin)?, &auth_mint_zkbin);
        let fee_circuit = ZkCircuit::new(empty_witnesses(&fee_zkbin)?, &fee_zkbin);

        // Creating TokenMint, AuthTokenMint and Fee circuits proving keys
        let mint_pk = ProvingKey::build(mint_zkbin.k, &mint_circuit);
        let auth_mint_pk = ProvingKey::build(auth_mint_zkbin.k, &auth_mint_circuit);
        let fee_pk = ProvingKey::build(fee_zkbin.k, &fee_circuit);

        // Build the coin attributes
        let coin_attrs = CoinAttributes {
            public_key: recipient,
            value: amount,
            token_id,
            spend_hook: spend_hook.unwrap_or(FuncId::none()),
            user_data: user_data.unwrap_or(pallas::Base::ZERO),
            blind: Blind::random(&mut OsRng),
        };

        // Create the auth call
        let builder = AuthTokenMintCallBuilder {
            coin_attrs: coin_attrs.clone(),
            token_attrs: token_attrs.clone(),
            mint_keypair: mint_authority,
            auth_mint_zkbin,
            auth_mint_pk,
        };
        let auth_debris = builder.build()?;
        let mut data = vec![MoneyFunction::AuthTokenMintV1 as u8];
        auth_debris.params.encode_async(&mut data).await?;
        let auth_call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };

        // Create the minting call
        let builder = TokenMintCallBuilder { coin_attrs, token_attrs, mint_zkbin, mint_pk };
        let mint_debris = builder.build()?;
        let mut data = vec![MoneyFunction::TokenMintV1 as u8];
        mint_debris.params.encode_async(&mut data).await?;
        let mint_call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };

        // Create the TransactionBuilder containing above calls
        let mut tx_builder = TransactionBuilder::new(
            ContractCallLeaf { call: mint_call, proofs: mint_debris.proofs },
            vec![DarkTree::new(
                ContractCallLeaf { call: auth_call, proofs: auth_debris.proofs },
                vec![],
                None,
                None,
            )],
        )?;

        // We first have to execute the fee-less tx to gather its used gas, and then we feed
        // it into the fee-creating function.
        let mut tx = tx_builder.build()?;
        let auth_sigs = tx.create_sigs(&[mint_authority.secret])?;
        let mint_sigs = tx.create_sigs(&[])?;
        tx.signatures = vec![auth_sigs, mint_sigs];

        let tree = self.get_money_tree().await?;
        let (fee_call, fee_proofs, fee_secrets) =
            self.append_fee_call(&tx, &tree, &fee_pk, &fee_zkbin, None).await?;

        // Append the fee call to the transaction
        tx_builder.append(ContractCallLeaf { call: fee_call, proofs: fee_proofs }, vec![])?;

        // Now build the actual transaction and sign it with all necessary keys.
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&[mint_authority.secret])?;
        tx.signatures.push(sigs);
        let sigs = tx.create_sigs(&[])?;
        tx.signatures.push(sigs);
        let sigs = tx.create_sigs(&fee_secrets)?;
        tx.signatures.push(sigs);

        Ok(tx)
    }

    /// Create a token freeze transaction. Returns the transaction object on success.
    pub async fn freeze_token(&self, token_id: TokenId) -> Result<Transaction> {
        // Grab token ID mint authority and attributes
        let token_mint_authority = self.get_token_mint_authority(&token_id).await?;
        let token_attrs =
            self.derive_token_attributes(token_mint_authority.1, token_mint_authority.2);
        let mint_authority = Keypair::new(token_mint_authority.1);

        // Sanity check
        assert_eq!(token_id, token_attrs.to_token_id());

        // Now we need to do a lookup for the zkas proof bincodes, and create
        // the circuit objects and proving keys so we can build the transaction.
        // We also do this through the RPC.
        let zkas_bins = self.lookup_zkas(&MONEY_CONTRACT_ID).await?;

        let Some(auth_mint_zkbin) =
            zkas_bins.iter().find(|x| x.0 == MONEY_CONTRACT_ZKAS_AUTH_TOKEN_MINT_NS_V1)
        else {
            return Err(Error::Custom("Auth token mint circuit not found".to_string()))
        };

        let Some(fee_zkbin) = zkas_bins.iter().find(|x| x.0 == MONEY_CONTRACT_ZKAS_FEE_NS_V1)
        else {
            return Err(Error::Custom("Fee circuit not found".to_string()))
        };

        let auth_mint_zkbin = ZkBinary::decode(&auth_mint_zkbin.1, false)?;
        let fee_zkbin = ZkBinary::decode(&fee_zkbin.1, false)?;

        let auth_mint_circuit =
            ZkCircuit::new(empty_witnesses(&auth_mint_zkbin)?, &auth_mint_zkbin);
        let fee_circuit = ZkCircuit::new(empty_witnesses(&fee_zkbin)?, &fee_zkbin);

        // Creating AuthTokenMint and Fee circuits proving keys
        let auth_mint_pk = ProvingKey::build(auth_mint_zkbin.k, &auth_mint_circuit);
        let fee_pk = ProvingKey::build(fee_zkbin.k, &fee_circuit);

        // Create the freeze call
        let builder = AuthTokenFreezeCallBuilder {
            mint_keypair: mint_authority,
            token_attrs,
            auth_mint_zkbin,
            auth_mint_pk,
        };
        let freeze_debris = builder.build()?;
        let mut data = vec![MoneyFunction::AuthTokenFreezeV1 as u8];
        freeze_debris.params.encode_async(&mut data).await?;
        let freeze_call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };

        // Create the TransactionBuilder containing above call
        let mut tx_builder = TransactionBuilder::new(
            ContractCallLeaf { call: freeze_call, proofs: freeze_debris.proofs },
            vec![],
        )?;

        // We first have to execute the fee-less tx to gather its used gas, and then we feed
        // it into the fee-creating function.
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&[mint_authority.secret])?;
        tx.signatures.push(sigs);

        let tree = self.get_money_tree().await?;
        let (fee_call, fee_proofs, fee_secrets) =
            self.append_fee_call(&tx, &tree, &fee_pk, &fee_zkbin, None).await?;

        // Append the fee call to the transaction
        tx_builder.append(ContractCallLeaf { call: fee_call, proofs: fee_proofs }, vec![])?;

        // Now build the actual transaction and sign it with all necessary keys.
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&[mint_authority.secret])?;
        tx.signatures.push(sigs);
        let sigs = tx.create_sigs(&fee_secrets)?;
        tx.signatures.push(sigs);

        Ok(tx)
    }
}
