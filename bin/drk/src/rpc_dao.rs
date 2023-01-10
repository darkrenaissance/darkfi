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
    DAO_DAOS_COL_CALL_INDEX, DAO_DAOS_COL_DAO_ID, DAO_DAOS_COL_GOV_TOKEN_ID,
    DAO_DAOS_COL_LEAF_POSITION, DAO_DAOS_COL_NAME, DAO_DAOS_COL_PROPOSER_LIMIT,
    DAO_DAOS_COL_QUORUM, DAO_DAOS_COL_SECRET, DAO_DAOS_COL_TX_HASH, DAO_DAOS_TABLE,
};
use darkfi_serial::{deserialize, serialize};
use serde_json::json;

use super::Drk;
use crate::{dao::Dao, DaoParams};

impl Drk {
    /// Import given DAO into the wallet
    pub async fn dao_import(&self, dao_name: String, dao_params: DaoParams) -> Result<()> {
        // First let's check if we've imported this DAO before. We use the name
        // as the identifier.
        let query = format!("SELECT {} FROM {}", DAO_DAOS_COL_NAME, DAO_DAOS_TABLE);
        let params = json!([query, QueryType::Blob as u8, DAO_DAOS_COL_NAME]);
        let req = JsonRequest::new("wallet.query_row_multi", params);
        let rep = self.rpc_client.request(req).await?;

        // The returned thing should be an array of found rows.
        let Some(rows) = rep.as_array() else {
            return Err(anyhow!("Unexpected response from darkfid: {}", rep))
        };

        for row in rows {
            let name_bytes: Vec<u8> = serde_json::from_value(row[0].clone())?;
            let name: String = deserialize(&name_bytes)?;
            if name == dao_name {
                return Err(anyhow!("DAO \"{}\" already imported in wallet", dao_name))
            }
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

    async fn dao_get_by_id(&self, dao_id: u64) -> Result<Dao> {
        let query =
            format!("SELECT * FROM {} WHERE {} = {}", DAO_DAOS_TABLE, DAO_DAOS_COL_DAO_ID, dao_id);

        let params = json!([
            query,
            QueryType::Integer as u8,
            DAO_DAOS_COL_DAO_ID,
            QueryType::Blob as u8,
            DAO_DAOS_COL_NAME,
            QueryType::Integer as u8,
            DAO_DAOS_COL_PROPOSER_LIMIT,
            QueryType::Integer as u8,
            DAO_DAOS_COL_QUORUM,
            QueryType::Integer as u8,
            DAO_DAOS_COL_APPROVAL_RATIO_BASE,
            QueryType::Integer as u8,
            DAO_DAOS_COL_APPROVAL_RATIO_QUOT,
            QueryType::Blob as u8,
            DAO_DAOS_COL_GOV_TOKEN_ID,
            QueryType::Blob as u8,
            DAO_DAOS_COL_SECRET,
            QueryType::Blob as u8,
            DAO_DAOS_COL_BULLA_BLIND,
            QueryType::OptionBlob as u8,
            DAO_DAOS_COL_LEAF_POSITION,
            QueryType::OptionBlob as u8,
            DAO_DAOS_COL_TX_HASH,
            QueryType::OptionInteger as u8,
            DAO_DAOS_COL_CALL_INDEX,
        ]);

        let req = JsonRequest::new("wallet.query_row_single", params);
        let rep = self.rpc_client.request(req).await?;

        let Some(row) = rep.as_array() else {
            return Err(anyhow!("Unexpected response from darkfid: {}", rep));
        };

        let dao_id: u64 = serde_json::from_value(row[0].clone())?;

        let name_bytes: Vec<u8> = serde_json::from_value(row[1].clone())?;
        let name = deserialize(&name_bytes)?;
        let proposer_limit = serde_json::from_value(row[2].clone())?;
        let quorum = serde_json::from_value(row[3].clone())?;
        let approval_ratio_base = serde_json::from_value(row[4].clone())?;
        let approval_ratio_quot = serde_json::from_value(row[5].clone())?;
        let gov_token_bytes: Vec<u8> = serde_json::from_value(row[6].clone())?;
        let gov_token_id = deserialize(&gov_token_bytes)?;
        let secret_bytes: Vec<u8> = serde_json::from_value(row[7].clone())?;
        let secret_key = deserialize(&secret_bytes)?;
        let bulla_blind_bytes: Vec<u8> = serde_json::from_value(row[8].clone())?;
        let bulla_blind = deserialize(&bulla_blind_bytes)?;

        let leaf_position_bytes: Vec<u8> = serde_json::from_value(row[9].clone())?;
        let tx_hash_bytes: Vec<u8> = serde_json::from_value(row[10].clone())?;
        let call_index = serde_json::from_value(row[11].clone())?;

        let leaf_position = if leaf_position_bytes.is_empty() {
            None
        } else {
            Some(deserialize(&leaf_position_bytes)?)
        };

        let tx_hash =
            if tx_hash_bytes.is_empty() { None } else { Some(deserialize(&tx_hash_bytes)?) };

        Ok(Dao {
            name,
            proposer_limit,
            quorum,
            approval_ratio_base,
            approval_ratio_quot,
            gov_token_id,
            secret_key,
            bulla_blind,
            leaf_position,
            tx_hash,
            call_index,
        })
    }

    async fn dao_list_single(&self, dao_id: u64) -> Result<()> {
        let dao = self.dao_get_by_id(dao_id).await?;

        println!("DAO Parameters:");
        println!("Name: {}", dao.name);
        println!("Proposer limit: {}", dao.proposer_limit);
        println!("Quorum: {}", dao.quorum);
        println!(
            "Approval ratio: {}",
            dao.approval_ratio_base as f64 / dao.approval_ratio_quot as f64
        );
        println!("Governance token ID: {}", dao.gov_token_id);
        println!("Secret key: {}", dao.secret_key);
        println!("Bulla blind: {:?}", dao.bulla_blind);
        println!("Leaf position: {:?}", dao.leaf_position);
        println!("Tx hash: {:?}", dao.tx_hash);
        println!("Call idx: {:?}", dao.call_index);

        Ok(())
    }

    /// List DAO(s) imported in the wallet
    pub async fn dao_list(&self, dao_id: Option<u64>) -> Result<()> {
        if dao_id.is_some() {
            return self.dao_list_single(dao_id.unwrap()).await
        }

        let query = format!(
            "SELECT {}, {} FROM {}",
            DAO_DAOS_COL_DAO_ID, DAO_DAOS_COL_NAME, DAO_DAOS_TABLE
        );

        let params = json!([
            query,
            QueryType::Integer as u8,
            DAO_DAOS_COL_DAO_ID,
            QueryType::Blob as u8,
            DAO_DAOS_COL_NAME
        ]);

        let req = JsonRequest::new("wallet.query_row_multi", params);
        let rep = self.rpc_client.request(req).await?;

        let Some(rows) = rep.as_array() else {
            return Err(anyhow!("Unexpected response from darkfid: {}", rep))
        };

        for row in rows {
            let dao_id: u64 = serde_json::from_value(row[0].clone())?;
            let dao_name_bytes: Vec<u8> = serde_json::from_value(row[1].clone())?;
            let dao_name: String = deserialize(&dao_name_bytes)?;
            println!("[{}] {}", dao_id, dao_name);
        }

        Ok(())
    }

    /// Mint a DAO on-chain
    pub async fn dao_mint(&self, dao_id: u64) -> Result<()> {
        let dao = self.dao_get_by_id(dao_id).await?;

        Ok(())
    }
}
