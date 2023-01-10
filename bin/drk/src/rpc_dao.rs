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
use darkfi_dao_contract::dao_client::{
    DAO_DAOS_COL_APPROVAL_RATIO_BASE, DAO_DAOS_COL_APPROVAL_RATIO_QUOT, DAO_DAOS_COL_BULLA_BLIND,
    DAO_DAOS_COL_DAO_ID, DAO_DAOS_COL_GOV_TOKEN_ID, DAO_DAOS_COL_NAME, DAO_DAOS_COL_PROPOSER_LIMIT,
    DAO_DAOS_COL_QUORUM, DAO_DAOS_COL_SECRET, DAO_DAOS_TABLE,
};
use darkfi_serial::serialize;
use serde_json::json;

use super::Drk;
use crate::DaoParams;

impl Drk {
    /// Import given DAO into the wallet
    pub async fn dao_import(&self, dao_name: String, dao_params: DaoParams) -> Result<()> {
        // First let's check if we've imported this DAO before. We use the name
        // as the identifier.
        let query = format!(
            "SELECT {} FROM {} WHERE {} = {}",
            DAO_DAOS_COL_DAO_ID, DAO_DAOS_TABLE, DAO_DAOS_COL_NAME, dao_name
        );
        let params = json!([query, QueryType::Integer as u8, DAO_DAOS_COL_DAO_ID]);
        let req = JsonRequest::new("wallet.query_row_single", params);

        if (self.rpc_client.request(req).await).is_ok() {
            return Err(anyhow!("DAO \"{}\" already imported in wallet.", dao_name))
        }

        eprintln!("Importing \"{}\" DAO into wallet", dao_name);

        let query = format!(
            "INSERT INTO {} ({}, {}, {}, {}, {}, {}, {}, {}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8);",
            DAO_DAOS_TABLE, DAO_DAOS_COL_NAME, DAO_DAOS_COL_PROPOSER_LIMIT,
            DAO_DAOS_COL_QUORUM, DAO_DAOS_COL_APPROVAL_RATIO_BASE, DAO_DAOS_COL_APPROVAL_RATIO_QUOT,
            DAO_DAOS_COL_GOV_TOKEN_ID, DAO_DAOS_COL_SECRET, DAO_DAOS_COL_BULLA_BLIND,
        );

        let params = json!([
            query,
            QueryType::Blob as u8,
            serialize(&dao_name),
            QueryType::Integer as u8,
            dao_params.proposer_limit,
            QueryType::Integer as u8,
            dao_params.quorum,
            QueryType::Integer as u8,
            dao_params.approval_ratio_base,
            QueryType::Integer as u8,
            dao_params.approval_ratio_quot,
            QueryType::Blob as u8,
            serialize(&dao_params.gov_token_id),
            QueryType::Blob as u8,
            serialize(&dao_params.secret_key),
            QueryType::Blob as u8,
            serialize(&dao_params.bulla_blind),
        ]);

        eprintln!("Executing JSON-RPC request to add DAO to wallet");
        let req = JsonRequest::new("wallet.exec_sql", params);
        self.rpc_client.request(req).await?;
        eprintln!("DAO imported successfully");

        Ok(())
    }
}
