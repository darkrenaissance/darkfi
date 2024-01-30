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

use rand::rngs::OsRng;
use rusqlite::types::Value;

use darkfi::{
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    util::parse::decode_base10,
    zk::{proof::ProvingKey, vm::ZkCircuit, vm_heap::empty_witnesses},
    zkas::ZkBinary,
    Error, Result,
};
use darkfi_money_contract::{
    client::{token_freeze_v1::TokenFreezeCallBuilder, token_mint_v1::TokenMintCallBuilder},
    MoneyFunction, MONEY_CONTRACT_ZKAS_TOKEN_FRZ_NS_V1, MONEY_CONTRACT_ZKAS_TOKEN_MINT_NS_V1,
};
use darkfi_sdk::{
    crypto::{contract_id::MONEY_CONTRACT_ID, Keypair, PublicKey, SecretKey, TokenId},
    pasta::pallas,
    tx::ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable};

use crate::{
    error::WalletDbResult,
    money::{
        BALANCE_BASE10_DECIMALS, MONEY_TOKENS_COL_IS_FROZEN, MONEY_TOKENS_COL_MINT_AUTHORITY,
        MONEY_TOKENS_COL_TOKEN_ID, MONEY_TOKENS_TABLE,
    },
    Drk,
};

impl Drk {
    /// Import a token mint authority into the wallet
    pub async fn import_mint_authority(&self, mint_authority: SecretKey) -> WalletDbResult<()> {
        let token_id = TokenId::derive(mint_authority);
        let is_frozen = 0;

        let query = format!(
            "INSERT INTO {} ({}, {}, {}) VALUES (?1, ?2, ?3);",
            *MONEY_TOKENS_TABLE,
            MONEY_TOKENS_COL_MINT_AUTHORITY,
            MONEY_TOKENS_COL_TOKEN_ID,
            MONEY_TOKENS_COL_IS_FROZEN,
        );

        self.wallet
            .exec_sql(
                &query,
                rusqlite::params![serialize(&mint_authority), serialize(&token_id), is_frozen,],
            )
            .await
    }

    pub async fn list_tokens(&self) -> Result<Vec<(TokenId, SecretKey, bool)>> {
        let rows = match self.wallet.query_multiple(&MONEY_TOKENS_TABLE, &[], &[]).await {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::RusqliteError(format!(
                    "[list_tokens] Tokens retrieval failed: {e:?}"
                )))
            }
        };

        let mut ret = Vec::with_capacity(rows.len());
        for row in rows {
            let Value::Blob(ref auth_bytes) = row[0] else {
                return Err(Error::ParseFailed("[list_tokens] Mint authority bytes parsing failed"))
            };
            let mint_authority = deserialize(auth_bytes)?;

            let Value::Blob(ref token_bytes) = row[1] else {
                return Err(Error::ParseFailed("[list_tokens] Token ID bytes parsing failed"))
            };
            let token_id = deserialize(token_bytes)?;

            let Value::Integer(frozen) = row[2] else {
                return Err(Error::ParseFailed("[list_tokens] Is frozen parsing failed"))
            };
            let Ok(frozen) = i32::try_from(frozen) else {
                return Err(Error::ParseFailed("[list_tokens] Is frozen parsing failed"))
            };

            ret.push((token_id, mint_authority, frozen != 0));
        }

        Ok(ret)
    }

    /// Create a token mint transaction. Returns the transaction object on success.
    pub async fn mint_token(
        &self,
        amount: &str,
        recipient: PublicKey,
        token_id: TokenId,
    ) -> Result<Transaction> {
        // TODO: Mint directly into DAO treasury
        let spend_hook = pallas::Base::zero();
        let user_data = pallas::Base::zero();

        let amount = decode_base10(amount, BALANCE_BASE10_DECIMALS, false)?;

        let mut tokens = self.list_tokens().await?;
        tokens.retain(|x| x.0 == token_id);
        if tokens.is_empty() {
            return Err(Error::Custom(format!(
                "Did not find mint authority for token ID {token_id}"
            )))
        }
        assert!(tokens.len() == 1);

        let mint_authority = Keypair::new(tokens[0].1);

        if tokens[0].2 {
            return Err(Error::Custom(
                "This token mint is marked as frozen in the wallet".to_string(),
            ))
        }

        // Now we need to do a lookup for the zkas proof bincodes, and create
        // the circuit objects and proving keys so we can build the transaction.
        // We also do this through the RPC.
        let zkas_bins = self.lookup_zkas(&MONEY_CONTRACT_ID).await?;
        let zkas_ns = MONEY_CONTRACT_ZKAS_TOKEN_MINT_NS_V1;

        let Some(token_mint_zkbin) = zkas_bins.iter().find(|x| x.0 == zkas_ns) else {
            return Err(Error::Custom("Token mint circuit not found".to_string()))
        };

        let token_mint_zkbin = ZkBinary::decode(&token_mint_zkbin.1)?;
        let token_mint_circuit =
            ZkCircuit::new(empty_witnesses(&token_mint_zkbin)?, &token_mint_zkbin);

        eprintln!("Creating token mint circuit proving keys");
        let token_mint_pk = ProvingKey::build(token_mint_zkbin.k, &token_mint_circuit);
        let mint_builder = TokenMintCallBuilder {
            mint_authority,
            recipient,
            amount,
            spend_hook,
            user_data,
            token_mint_zkbin,
            token_mint_pk,
        };

        eprintln!("Building transaction parameters");
        let debris = mint_builder.build()?;

        // Encode and sign the transaction
        let mut data = vec![MoneyFunction::TokenMintV1 as u8];
        debris.params.encode(&mut data)?;
        let call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };
        let mut tx_builder =
            TransactionBuilder::new(ContractCallLeaf { call, proofs: debris.proofs }, vec![])?;
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&mut OsRng, &[mint_authority.secret])?;
        tx.signatures = vec![sigs];

        Ok(tx)
    }

    /// Create a token freeze transaction. Returns the transaction object on success.
    pub async fn freeze_token(&self, token_id: TokenId) -> Result<Transaction> {
        let mut tokens = self.list_tokens().await?;
        tokens.retain(|x| x.0 == token_id);
        if tokens.is_empty() {
            return Err(Error::Custom(format!(
                "Did not find mint authority for token ID {token_id}"
            )))
        }
        assert!(tokens.len() == 1);

        let mint_authority = Keypair::new(tokens[0].1);

        if tokens[0].2 {
            return Err(Error::Custom(
                "This token is already marked as frozen in the wallet".to_string(),
            ))
        }

        let zkas_bins = self.lookup_zkas(&MONEY_CONTRACT_ID).await?;
        let zkas_ns = MONEY_CONTRACT_ZKAS_TOKEN_FRZ_NS_V1;

        let Some(token_freeze_zkbin) = zkas_bins.iter().find(|x| x.0 == zkas_ns) else {
            return Err(Error::Custom("Token freeze circuit not found".to_string()))
        };

        let token_freeze_zkbin = ZkBinary::decode(&token_freeze_zkbin.1)?;
        let token_freeze_circuit =
            ZkCircuit::new(empty_witnesses(&token_freeze_zkbin)?, &token_freeze_zkbin);

        eprintln!("Creating token freeze circuit proving keys");
        let token_freeze_pk = ProvingKey::build(token_freeze_zkbin.k, &token_freeze_circuit);
        let freeze_builder =
            TokenFreezeCallBuilder { mint_authority, token_freeze_zkbin, token_freeze_pk };

        eprintln!("Building transaction parameters");
        let debris = freeze_builder.build()?;

        // Encode and sign the transaction
        let mut data = vec![MoneyFunction::TokenFreezeV1 as u8];
        debris.params.encode(&mut data)?;
        let call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };
        let mut tx_builder =
            TransactionBuilder::new(ContractCallLeaf { call, proofs: debris.proofs }, vec![])?;
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&mut OsRng, &[mint_authority.secret])?;
        tx.signatures = vec![sigs];

        Ok(tx)
    }
}
