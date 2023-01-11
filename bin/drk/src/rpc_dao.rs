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
    wallet::walletdb::QueryType,
    zk::{empty_witnesses, halo2::Field, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use darkfi_dao_contract::{
    dao_client,
    dao_client::{
        DaoInfo, DAO_DAOS_COL_APPROVAL_RATIO_BASE, DAO_DAOS_COL_APPROVAL_RATIO_QUOT,
        DAO_DAOS_COL_BULLA_BLIND, DAO_DAOS_COL_GOV_TOKEN_ID, DAO_DAOS_COL_NAME,
        DAO_DAOS_COL_PROPOSER_LIMIT, DAO_DAOS_COL_QUORUM, DAO_DAOS_COL_SECRET, DAO_DAOS_TABLE,
    },
    DaoFunction, DAO_CONTRACT_ZKAS_DAO_MINT_NS, DAO_CONTRACT_ZKAS_DAO_PROPOSE_BURN_NS,
    DAO_CONTRACT_ZKAS_DAO_PROPOSE_MAIN_NS,
};
use darkfi_money_contract::client::OwnCoin;
use darkfi_sdk::{
    crypto::{PublicKey, SecretKey, TokenId, DAO_CONTRACT_ID},
    incrementalmerkletree::Tree,
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
        recipient: PublicKey,
        amount: u64,
        token_id: TokenId,
        serial: pallas::Base,
    ) -> Result<Transaction> {
        let daos = self.wallet_get_daos().await?;
        let Some(dao) = daos.get(dao_id as usize - 1) else {
            return Err(anyhow!("DAO not found in wallet"))
        };

        if dao.leaf_position.is_none() || dao.tx_hash.is_none() {
            return Err(anyhow!("DAO seems to not have been deployed yet"))
        }

        let bulla = dao.bulla();
        let owncoins = self.wallet_coins(false).await?;

        let mut dao_owncoins: Vec<OwnCoin> = owncoins.iter().map(|x| x.0.clone()).collect();
        dao_owncoins.retain(|x| {
            x.note.token_id == token_id &&
                x.note.spend_hook == DAO_CONTRACT_ID.inner() &&
                x.note.user_data == bulla.inner()
        });

        let mut gov_owncoins: Vec<OwnCoin> = owncoins.iter().map(|x| x.0.clone()).collect();
        gov_owncoins.retain(|x| x.note.token_id == dao.gov_token_id);

        if dao_owncoins.is_empty() {
            return Err(anyhow!("Did not find any {} coins owned by this DAO", token_id))
        }

        if gov_owncoins.is_empty() {
            return Err(anyhow!("Did not find any governance {} coins in wallet", dao.gov_token_id))
        }

        if dao_owncoins.iter().map(|x| x.note.value).sum::<u64>() < amount {
            return Err(anyhow!("Not enough DAO balance for token ID: {}", token_id))
        }

        if gov_owncoins.iter().map(|x| x.note.value).sum::<u64>() < dao.proposer_limit {
            return Err(anyhow!("Not enough gov token {} balance to propose", dao.gov_token_id))
        }

        // FIXME: Here we're looking for a coin == proposer_limit but this shouldn't have to
        // be the case {
        let Some(gov_coin) = gov_owncoins.iter().find(|x| x.note.value == dao.proposer_limit) else {
            return Err(anyhow!("Did not find a single gov coin of value {}", dao.proposer_limit));
        };
        // }

        // Lookup the zkas bins
        let zkas_bins = self.lookup_zkas(&DAO_CONTRACT_ID).await?;
        let Some(propose_burn_zkbin) =
            zkas_bins.iter().find(|x| x.0 == DAO_CONTRACT_ZKAS_DAO_PROPOSE_BURN_NS) else
        {
            return Err(anyhow!("Propose Burn circuit not found"))
        };

        let Some(propose_main_zkbin) =
            zkas_bins.iter().find(|x| x.0 == DAO_CONTRACT_ZKAS_DAO_PROPOSE_MAIN_NS) else
        {
            return Err(anyhow!("Propose Main circuit not found"))
        };

        let propose_burn_zkbin = ZkBinary::decode(&propose_burn_zkbin.1)?;
        let propose_main_zkbin = ZkBinary::decode(&propose_main_zkbin.1)?;

        let k = 13;
        let propose_burn_circuit =
            ZkCircuit::new(empty_witnesses(&propose_burn_zkbin), propose_burn_zkbin.clone());
        let propose_main_circuit =
            ZkCircuit::new(empty_witnesses(&propose_main_zkbin), propose_main_zkbin.clone());

        eprintln!("Creating Propose Burn circuit proving key");
        let propose_burn_pk = ProvingKey::build(k, &propose_burn_circuit);
        eprintln!("Creating Propose Main circuit proving key");
        let propose_main_pk = ProvingKey::build(k, &propose_main_circuit);

        // Now create the parameters for the proposal tx
        let signature_secret = SecretKey::random(&mut OsRng);

        // Get the Merkle path for the gov coin in the money tree
        let money_merkle_tree = self.wallet_tree().await?;
        let root = money_merkle_tree.root(0).unwrap();
        let gov_coin_merkle_path =
            money_merkle_tree.authentication_path(gov_coin.leaf_position, &root).unwrap();

        // Fetch the daos Merkle tree
        let (daos_tree, _) = self.wallet_dao_trees().await?;

        let input = dao_client::DaoProposeStakeInput {
            secret: gov_coin.secret, // <-- TODO: Is this correct?
            note: gov_coin.note.clone(),
            leaf_position: gov_coin.leaf_position,
            merkle_path: gov_coin_merkle_path,
            signature_secret,
        };

        let (dao_merkle_path, dao_merkle_root) = {
            let root = daos_tree.root(0).unwrap();
            let dao_merkle_path =
                daos_tree.authentication_path(dao.leaf_position.unwrap(), &root).unwrap();
            (dao_merkle_path, root)
        };

        let proposal_blind = pallas::Base::random(&mut OsRng);
        let proposal = dao_client::DaoProposalInfo {
            dest: recipient,
            amount,
            serial,
            token_id,
            blind: proposal_blind,
        };

        let daoinfo = DaoInfo {
            proposer_limit: dao.proposer_limit,
            quorum: dao.quorum,
            approval_ratio_quot: dao.approval_ratio_quot,
            approval_ratio_base: dao.approval_ratio_base,
            gov_token_id: dao.gov_token_id,
            public_key: PublicKey::from_secret(dao.secret_key),
            bulla_blind: dao.bulla_blind,
        };

        let call = dao_client::DaoProposeCall {
            inputs: vec![input],
            proposal,
            dao: daoinfo,
            dao_leaf_position: dao.leaf_position.unwrap(),
            dao_merkle_path,
            dao_merkle_root,
        };

        eprintln!("Creating ZK proofs...");
        let (params, proofs) = call.make(
            &propose_burn_zkbin,
            &propose_burn_pk,
            &propose_main_zkbin,
            &propose_main_pk,
        )?;

        let mut data = vec![DaoFunction::Propose as u8];
        params.encode(&mut data)?;
        let calls = vec![ContractCall { contract_id: *DAO_CONTRACT_ID, data }];
        let proofs = vec![proofs];
        let mut tx = Transaction { calls, proofs, signatures: vec![] };
        let sigs = tx.create_sigs(&mut OsRng, &vec![signature_secret])?;
        tx.signatures = vec![sigs];

        Ok(tx)
    }
}
