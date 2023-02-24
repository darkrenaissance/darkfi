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

use anyhow::{anyhow, Result};
use darkfi::{rpc::jsonrpc::JsonRequest, wallet::walletdb::QueryType};
use darkfi_money_contract::client::{
    MONEY_TOKENS_IS_FROZEN, MONEY_TOKENS_MINT_AUTHORITY, MONEY_TOKENS_TABLE, MONEY_TOKENS_TOKEN_ID,
};
use darkfi_sdk::crypto::{SecretKey, TokenId};
use darkfi_serial::{deserialize, serialize};
use serde_json::json;

use super::Drk;

impl Drk {
    /// Import a token mint authority into the wallet
    pub async fn import_mint_authority(&self, mint_authority: SecretKey) -> Result<()> {
        let token_id = TokenId::derive(mint_authority);
        let is_frozen = 0;

        let query = format!(
            "INSERT INTO {} ({}, {}, {}) VALUES (?1, ?2, ?3);",
            MONEY_TOKENS_TABLE,
            MONEY_TOKENS_MINT_AUTHORITY,
            MONEY_TOKENS_TOKEN_ID,
            MONEY_TOKENS_IS_FROZEN,
        );

        let params = json!([
            query,
            QueryType::Blob as u8,
            serialize(&mint_authority),
            QueryType::Blob as u8,
            serialize(&token_id),
            QueryType::Integer as u8,
            is_frozen,
        ]);

        let req = JsonRequest::new("wallet.exec_sql", params);
        let _ = self.rpc_client.request(req).await?;

        Ok(())
    }

    pub async fn list_tokens(&self) -> Result<Vec<(TokenId, SecretKey, bool)>> {
        let mut ret = vec![];

        let query = format!("SELECT * FROM {};", MONEY_TOKENS_TABLE);

        let params = json!([
            query,
            QueryType::Blob as u8,
            MONEY_TOKENS_MINT_AUTHORITY,
            QueryType::Blob as u8,
            MONEY_TOKENS_TOKEN_ID,
            QueryType::Integer as u8,
            MONEY_TOKENS_IS_FROZEN,
        ]);

        let req = JsonRequest::new("wallet.query_row_multi", params);
        let rep = self.rpc_client.request(req).await?;

        let Some(rows) = rep.as_array() else {
            return Err(anyhow!("[list_tokens] Unexpected response from darkfid: {}", rep));
        };

        for row in rows {
            let auth_bytes: Vec<u8> = serde_json::from_value(row[0].clone())?;
            let mint_authority = deserialize(&auth_bytes)?;

            let token_bytes: Vec<u8> = serde_json::from_value(row[1].clone())?;
            let token_id = deserialize(&token_bytes)?;

            let frozen: i32 = serde_json::from_value(row[2].clone())?;

            ret.push((token_id, mint_authority, frozen != 0));
        }

        Ok(ret)
    }
}
