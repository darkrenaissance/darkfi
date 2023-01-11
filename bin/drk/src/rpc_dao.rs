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
use darkfi::{
    rpc::jsonrpc::JsonRequest,
    tx::Transaction,
    util::parse::encode_base10,
    wallet::walletdb::QueryType,
    zk::{empty_witnesses, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use darkfi_dao_contract::{
    dao_client,
    dao_client::{
        DaoInfo, DAO_DAOS_COL_APPROVAL_RATIO_BASE, DAO_DAOS_COL_APPROVAL_RATIO_QUOT,
        DAO_DAOS_COL_BULLA_BLIND, DAO_DAOS_COL_GOV_TOKEN_ID, DAO_DAOS_COL_NAME,
        DAO_DAOS_COL_PROPOSER_LIMIT, DAO_DAOS_COL_QUORUM, DAO_DAOS_COL_SECRET, DAO_DAOS_TABLE,
    },
    DaoFunction, DAO_CONTRACT_ZKAS_DAO_MINT_NS,
};
use darkfi_money_contract::client::OwnCoin;
use darkfi_sdk::{
    crypto::{PublicKey, TokenId, DAO_CONTRACT_ID},
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable};
use rand::rngs::OsRng;
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
        let daos = self.wallet_get_daos().await?;

        let Some(dao) = daos.iter().find(|x| x.id == dao_id) else {
            return Err(anyhow!("DAO not found in wallet"))
        };

        Ok(dao.clone())
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

        let daos = self.wallet_get_daos().await?;

        for dao in daos {
            println!("[{}] {}", dao.id, dao.name);
        }

        Ok(())
    }

    /// Mint a DAO on-chain
    pub async fn dao_mint(&self, dao_id: u64) -> Result<Transaction> {
        let dao = self.dao_get_by_id(dao_id).await?;

        if dao.tx_hash.is_some() {
            return Err(anyhow!("This DAO seems to have already been minted on-chain"))
        }

        let dao_info = DaoInfo {
            proposer_limit: dao.proposer_limit,
            quorum: dao.quorum,
            approval_ratio_base: dao.approval_ratio_base,
            approval_ratio_quot: dao.approval_ratio_quot,
            gov_token_id: dao.gov_token_id,
            public_key: PublicKey::from_secret(dao.secret_key),
            bulla_blind: dao.bulla_blind,
        };

        let zkas_bins = self.lookup_zkas(&DAO_CONTRACT_ID).await?;
        let Some(dao_mint_zkbin) = zkas_bins.iter().find(|x| x.0 == DAO_CONTRACT_ZKAS_DAO_MINT_NS) else {
            return Err(anyhow!("DAO Mint circuit not found"));
        };

        let dao_mint_zkbin = ZkBinary::decode(&dao_mint_zkbin.1)?;
        let k = 13;
        let dao_mint_circuit =
            ZkCircuit::new(empty_witnesses(&dao_mint_zkbin), dao_mint_zkbin.clone());
        eprintln!("Creating DAO Mint proving key");
        let dao_mint_pk = ProvingKey::build(k, &dao_mint_circuit);

        let (params, proofs) =
            dao_client::make_mint_call(&dao_info, &dao.secret_key, &dao_mint_zkbin, &dao_mint_pk)?;

        let mut data = vec![DaoFunction::Mint as u8];
        params.encode(&mut data)?;
        let calls = vec![ContractCall { contract_id: *DAO_CONTRACT_ID, data }];
        let proofs = vec![proofs];
        let mut tx = Transaction { calls, proofs, signatures: vec![] };
        let sigs = tx.create_sigs(&mut OsRng, &[dao.secret_key])?;
        tx.signatures = vec![sigs];

        Ok(tx)
    }

    /// Create a DAO proposal
    pub async fn dao_propose(
        &self,
        dao_id: u64,
        rcpt: PublicKey,
        amount: u64,
        token_id: TokenId,
        serial: pallas::Base,
    ) -> Result<Transaction> {
        let daos = self.wallet_get_daos().await?;
        let Some(dao) = daos.get(dao_id as usize - 1) else {
            return Err(anyhow!("DAO not found in wallet"))
        };

        let bulla = dao.bulla();
        let owncoins = self.wallet_coins(false).await?;
        let mut dao_owncoins: Vec<OwnCoin> = owncoins.iter().map(|x| x.0.clone()).collect();
        dao_owncoins.retain(|x| {
            x.note.token_id == token_id &&
                x.note.spend_hook == DAO_CONTRACT_ID.inner() &&
                x.note.user_data == bulla.inner()
        });

        if dao_owncoins.is_empty() {
            return Err(anyhow!("Did not find any {} coins owned by this DAO", token_id))
        }

        let mut dao_balance = 0;
        for coin in dao_owncoins.iter() {
            dao_balance += coin.note.value;
        }

        if dao_balance < amount {
            return Err(anyhow!(
                "Not enough balance for token ID: {}, found: {}",
                token_id,
                encode_base10(dao_balance, 8)
            ))
        }

        let mut user_owncoins: Vec<OwnCoin> = owncoins.iter().map(|x| x.0.clone()).collect();
        user_owncoins.retain(|x| x.note.token_id == dao.gov_token_id);

        if user_owncoins.is_empty() {
            return Err(anyhow!("Did not find any governance {} coins in wallet", dao.gov_token_id))
        }

        let mut user_balance = 0;
        for coin in user_owncoins.iter() {
            user_balance += coin.note.value;
        }

        if user_balance < dao.proposer_limit {
            return Err(anyhow!(
                "Not enough governance token {} balance found to create proposal",
                dao.gov_token_id
            ))
        }

        todo!();
    }
}
