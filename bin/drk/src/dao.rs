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

use std::{collections::HashMap, fmt};

use lazy_static::lazy_static;
use num_bigint::BigUint;
use rand::rngs::OsRng;
use rusqlite::types::Value;

use darkfi::{
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    util::parse::{decode_base10, encode_base10},
    zk::{empty_witnesses, halo2::Field, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Error, Result,
};
use darkfi_dao_contract::{
    blockwindow,
    client::{make_mint_call, DaoProposeCall, DaoProposeStakeInput, DaoVoteCall, DaoVoteInput},
    model::{
        Dao, DaoAuthCall, DaoBulla, DaoMintParams, DaoProposal, DaoProposalBulla, DaoProposeParams,
        DaoVoteParams,
    },
    DaoFunction, DAO_CONTRACT_ZKAS_DAO_MINT_NS, DAO_CONTRACT_ZKAS_DAO_PROPOSE_INPUT_NS,
    DAO_CONTRACT_ZKAS_DAO_PROPOSE_MAIN_NS, DAO_CONTRACT_ZKAS_DAO_VOTE_INPUT_NS,
    DAO_CONTRACT_ZKAS_DAO_VOTE_MAIN_NS,
};
use darkfi_money_contract::{
    model::{CoinAttributes, TokenId},
    MoneyFunction, MONEY_CONTRACT_ZKAS_FEE_NS_V1,
};
use darkfi_sdk::{
    bridgetree,
    crypto::{
        smt::{MemoryStorageFp, PoseidonFp, SmtMemoryFp, EMPTY_NODES_FP},
        util::{fp_mod_fv, fp_to_u64},
        BaseBlind, Blind, FuncId, FuncRef, Keypair, MerkleNode, MerkleTree, PublicKey, ScalarBlind,
        SecretKey, DAO_CONTRACT_ID, MONEY_CONTRACT_ID,
    },
    pasta::pallas,
    tx::TransactionHash,
    ContractCall,
};
use darkfi_serial::{
    async_trait, deserialize_async, serialize_async, AsyncEncodable, SerialDecodable,
    SerialEncodable,
};

use crate::{
    convert_named_params,
    error::{WalletDbError, WalletDbResult},
    money::{BALANCE_BASE10_DECIMALS, MONEY_SMT_COL_KEY, MONEY_SMT_COL_VALUE, MONEY_SMT_TABLE},
    walletdb::{WalletSmt, WalletStorage},
    Drk,
};

// Wallet SQL table constant names. These have to represent the `wallet.sql`
// SQL schema. Table names are prefixed with the contract ID to avoid collisions.
lazy_static! {
    pub static ref DAO_DAOS_TABLE: String = format!("{}_dao_daos", DAO_CONTRACT_ID.to_string());
    pub static ref DAO_TREES_TABLE: String = format!("{}_dao_trees", DAO_CONTRACT_ID.to_string());
    pub static ref DAO_COINS_TABLE: String = format!("{}_dao_coins", DAO_CONTRACT_ID.to_string());
    pub static ref DAO_PROPOSALS_TABLE: String =
        format!("{}_dao_proposals", DAO_CONTRACT_ID.to_string());
    pub static ref DAO_VOTES_TABLE: String = format!("{}_dao_votes", DAO_CONTRACT_ID.to_string());
}

// DAO_DAOS_TABLE
pub const DAO_DAOS_COL_BULLA: &str = "bulla";
pub const DAO_DAOS_COL_NAME: &str = "name";
pub const DAO_DAOS_COL_PARAMS: &str = "params";
pub const DAO_DAOS_COL_LEAF_POSITION: &str = "leaf_position";
pub const DAO_DAOS_COL_TX_HASH: &str = "tx_hash";
pub const DAO_DAOS_COL_CALL_INDEX: &str = "call_index";

// DAO_TREES_TABLE
pub const DAO_TREES_COL_DAOS_TREE: &str = "daos_tree";
pub const DAO_TREES_COL_PROPOSALS_TREE: &str = "proposals_tree";

// DAO_PROPOSALS_TABLE
pub const DAO_PROPOSALS_COL_BULLA: &str = "bulla";
pub const DAO_PROPOSALS_COL_DAO_BULLA: &str = "dao_bulla";
pub const DAO_PROPOSALS_COL_PROPOSAL: &str = "proposal";
pub const DAO_PROPOSALS_COL_DATA: &str = "data";
pub const DAO_PROPOSALS_COL_LEAF_POSITION: &str = "leaf_position";
pub const DAO_PROPOSALS_COL_MONEY_SNAPSHOT_TREE: &str = "money_snapshot_tree";
pub const DAO_PROPOSALS_COL_NULLIFIERS_SMT_SNAPSHOT: &str = "nullifiers_smt_snapshot";
pub const DAO_PROPOSALS_COL_TX_HASH: &str = "tx_hash";
pub const DAO_PROPOSALS_COL_CALL_INDEX: &str = "call_index";
pub const DAO_PROPOSALS_COL_EXEC_TX_HASH: &str = "exec_tx_hash";

// DAO_VOTES_TABLE
pub const DAO_VOTES_COL_PROPOSAL_BULLA: &str = "proposal_bulla";
pub const DAO_VOTES_COL_VOTE_OPTION: &str = "vote_option";
pub const DAO_VOTES_COL_YES_VOTE_BLIND: &str = "yes_vote_blind";
pub const DAO_VOTES_COL_ALL_VOTE_VALUE: &str = "all_vote_value";
pub const DAO_VOTES_COL_ALL_VOTE_BLIND: &str = "all_vote_blind";
pub const DAO_VOTES_COL_TX_HASH: &str = "tx_hash";
pub const DAO_VOTES_COL_CALL_INDEX: &str = "call_index";

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
/// Parameters representing a DAO to be initialized
pub struct DaoParams {
    /// The on chain representation of the DAO
    pub dao: Dao,
    /// Secret key for the DAO
    pub secret_key: SecretKey,
}

impl DaoParams {
    pub fn new(
        proposer_limit: u64,
        quorum: u64,
        approval_ratio_base: u64,
        approval_ratio_quot: u64,
        gov_token_id: TokenId,
        secret_key: SecretKey,
        bulla_blind: BaseBlind,
    ) -> Self {
        let dao = Dao {
            proposer_limit,
            quorum,
            approval_ratio_base,
            approval_ratio_quot,
            gov_token_id,
            public_key: PublicKey::from_secret(secret_key),
            bulla_blind,
        };
        Self { dao, secret_key }
    }
}

impl fmt::Display for DaoParams {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = format!(
            "{}\n{}\n{}: {} ({})\n{}: {} ({})\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {:?}",
            "DAO Parameters",
            "==============",
            "Proposer limit",
            encode_base10(self.dao.proposer_limit, BALANCE_BASE10_DECIMALS),
            self.dao.proposer_limit,
            "Quorum",
            encode_base10(self.dao.quorum, BALANCE_BASE10_DECIMALS),
            self.dao.quorum,
            "Approval ratio",
            self.dao.approval_ratio_quot as f64 / self.dao.approval_ratio_base as f64,
            "Governance Token ID",
            self.dao.gov_token_id,
            "Public key",
            self.dao.public_key,
            "Secret key",
            self.secret_key,
            "Bulla blind",
            self.dao.bulla_blind,
        );

        write!(f, "{}", s)
    }
}

#[derive(Debug, Clone)]
/// Structure representing a `DAO_DAOS_TABLE` record.
pub struct DaoRecord {
    /// Name identifier for the DAO
    pub name: String,
    /// DAO parameters
    pub params: DaoParams,
    /// Leaf position of the DAO in the Merkle tree of DAOs
    pub leaf_position: Option<bridgetree::Position>,
    /// The transaction hash where the DAO was deployed
    pub tx_hash: Option<TransactionHash>,
    /// The call index in the transaction where the DAO was deployed
    pub call_index: Option<u8>,
}

impl DaoRecord {
    pub fn new(
        name: String,
        params: DaoParams,
        leaf_position: Option<bridgetree::Position>,
        tx_hash: Option<TransactionHash>,
        call_index: Option<u8>,
    ) -> Self {
        Self { name, params, leaf_position, tx_hash, call_index }
    }

    pub fn bulla(&self) -> DaoBulla {
        self.params.dao.to_bulla()
    }

    pub fn keypair(&self) -> Keypair {
        let public = PublicKey::from_secret(self.params.secret_key);
        Keypair { public, secret: self.params.secret_key }
    }
}

impl fmt::Display for DaoRecord {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let leaf_position = match self.leaf_position {
            Some(p) => format!("{p:?}"),
            None => "None".to_string(),
        };
        let tx_hash = match self.tx_hash {
            Some(t) => format!("{t}"),
            None => "None".to_string(),
        };
        let call_index = match self.call_index {
            Some(c) => format!("{c}"),
            None => "None".to_string(),
        };
        let s = format!(
            "{}\n{}\n{}: {}\n{}: {}\n{}: {} ({})\n{}: {} ({})\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}",
            "DAO Parameters",
            "==============",
            "Name",
            self.name,
            "Bulla",
            self.bulla(),
            "Proposer limit",
            encode_base10(self.params.dao.proposer_limit, BALANCE_BASE10_DECIMALS),
            self.params.dao.proposer_limit,
            "Quorum",
            encode_base10(self.params.dao.quorum, BALANCE_BASE10_DECIMALS),
            self.params.dao.quorum,
            "Approval ratio",
            self.params.dao.approval_ratio_quot as f64 / self.params.dao.approval_ratio_base as f64,
            "Governance Token ID",
            self.params.dao.gov_token_id,
            "Public key",
            self.params.dao.public_key,
            "Secret key",
            self.params.secret_key,
            "Bulla blind",
            self.params.dao.bulla_blind,
            "Leaf position",
            leaf_position,
            "Transaction hash",
            tx_hash,
            "Call index",
            call_index,
        );

        write!(f, "{}", s)
    }
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
/// Structure representing a `DAO_PROPOSALS_TABLE` record.
pub struct ProposalRecord {
    /// The on chain representation of the proposal
    pub proposal: DaoProposal,
    /// Plaintext proposal call data the members share between them
    pub data: Option<Vec<u8>>,
    /// Leaf position of the proposal in the Merkle tree of proposals
    pub leaf_position: Option<bridgetree::Position>,
    /// Money merkle tree snapshot for reproducing the snapshot Merkle root
    pub money_snapshot_tree: Option<MerkleTree>,
    /// Money nullifiers SMT snapshot for reproducing the snapshot Merkle root
    pub nullifiers_smt_snapshot: Option<HashMap<BigUint, pallas::Base>>,
    /// The transaction hash where the proposal was deployed
    pub tx_hash: Option<TransactionHash>,
    /// The call index in the transaction where the proposal was deployed
    pub call_index: Option<u8>,
    /// The transaction hash where the proposal was executed
    pub exec_tx_hash: Option<TransactionHash>,
}

impl ProposalRecord {
    pub fn bulla(&self) -> DaoProposalBulla {
        self.proposal.to_bulla()
    }
}

impl fmt::Display for ProposalRecord {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let leaf_position = match self.leaf_position {
            Some(p) => format!("{p:?}"),
            None => "None".to_string(),
        };
        let tx_hash = match self.tx_hash {
            Some(t) => format!("{t}"),
            None => "None".to_string(),
        };
        let call_index = match self.call_index {
            Some(c) => format!("{c}"),
            None => "None".to_string(),
        };

        let s = format!(
            "{}\n{}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {} ({})",
            "Proposal parameters",
            "===================",
            "Bulla",
            self.bulla(),
            "DAO Bulla",
            self.proposal.dao_bulla,
            "Proposal leaf position",
            leaf_position,
            "Proposal transaction hash",
            tx_hash,
            "Proposal call index",
            call_index,
            "Creation block window",
            self.proposal.creation_day,
            "Duration",
            self.proposal.duration_days,
            "Block windows"
        );

        write!(f, "{}", s)
    }
}

/// Auxiliary structure representing a parsed proposal information from
/// a transaction call data.
pub struct ParsedProposal {
    /// Proposal parameters
    pub params: DaoProposeParams,
    /// Money merkle tree snapshot
    pub money_tree: MerkleTree,
    /// Money nullifiers SMT snapshot
    pub nullifiers_smt: HashMap<BigUint, pallas::Base>,
    /// The transaction hash where the proposal was found
    pub tx_hash: TransactionHash,
    /// The call index in the transaction where the proposal was found
    pub call_idx: u8,
}

#[derive(Debug, Clone)]
/// Structure representing a `DAO_VOTES_TABLE` record.
pub struct VoteRecord {
    /// Numeric identifier for the vote
    pub id: u64,
    /// Bulla identifier of the proposal this vote is for
    pub proposal: DaoProposalBulla,
    /// The vote
    pub vote_option: bool,
    /// Blinding factor for the yes vote
    pub yes_vote_blind: ScalarBlind,
    /// Value of all votes
    pub all_vote_value: u64,
    /// Blinding facfor of all votes
    pub all_vote_blind: ScalarBlind,
    /// Transaction hash where this vote was casted
    pub tx_hash: TransactionHash,
    /// call index in the transaction where this vote was casted
    pub call_index: u8,
}

impl Drk {
    /// Initialize wallet with tables for the DAO contract.
    pub async fn initialize_dao(&self) -> WalletDbResult<()> {
        // Initialize DAO wallet schema
        let wallet_schema = include_str!("../dao.sql");
        self.wallet.exec_batch_sql(wallet_schema)?;

        // Check if we have to initialize the Merkle trees.
        // We check if one exists, but we actually create two. This should be written
        // a bit better and safer.
        // For now, on success, we don't care what's returned, but in the future
        // we should actually check it.
        if self.wallet.query_single(&DAO_TREES_TABLE, &[DAO_TREES_COL_DAOS_TREE], &[]).is_err() {
            println!("Initializing DAO Merkle trees");
            let tree = MerkleTree::new(1);
            self.put_dao_trees(&tree, &tree).await?;
            println!("Successfully initialized Merkle trees for the DAO contract");
        }

        Ok(())
    }

    /// Replace the DAO Merkle trees in the wallet.
    pub async fn put_dao_trees(
        &self,
        daos_tree: &MerkleTree,
        proposals_tree: &MerkleTree,
    ) -> WalletDbResult<()> {
        // First we remove old records
        let query = format!("DELETE FROM {};", *DAO_TREES_TABLE);
        self.wallet.exec_sql(&query, &[])?;

        // then we insert the new one
        let query = format!(
            "INSERT INTO {} ({}, {}) VALUES (?1, ?2);",
            *DAO_TREES_TABLE, DAO_TREES_COL_DAOS_TREE, DAO_TREES_COL_PROPOSALS_TREE,
        );
        self.wallet.exec_sql(
            &query,
            rusqlite::params![
                serialize_async(daos_tree).await,
                serialize_async(proposals_tree).await
            ],
        )
    }

    /// Fetch DAO Merkle trees from the wallet.
    pub async fn get_dao_trees(&self) -> Result<(MerkleTree, MerkleTree)> {
        let row = match self.wallet.query_single(&DAO_TREES_TABLE, &[], &[]) {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::RusqliteError(format!(
                    "[get_dao_trees] Trees retrieval failed: {e:?}"
                )))
            }
        };

        let Value::Blob(ref daos_tree_bytes) = row[0] else {
            return Err(Error::ParseFailed("[get_dao_trees] DAO tree bytes parsing failed"))
        };
        let daos_tree = deserialize_async(daos_tree_bytes).await?;

        let Value::Blob(ref proposals_tree_bytes) = row[1] else {
            return Err(Error::ParseFailed("[get_dao_trees] Proposals tree bytes parsing failed"))
        };
        let proposals_tree = deserialize_async(proposals_tree_bytes).await?;

        Ok((daos_tree, proposals_tree))
    }

    /// Fetch all DAO secret keys from the wallet.
    pub async fn get_dao_secrets(&self) -> Result<Vec<SecretKey>> {
        let daos = self.get_daos().await?;
        let mut ret = Vec::with_capacity(daos.len());
        for dao in daos {
            ret.push(dao.params.secret_key);
        }

        Ok(ret)
    }

    /// Auxiliary function to parse a `DAO_DAOS_TABLE` record.
    async fn parse_dao_record(&self, row: &[Value]) -> Result<DaoRecord> {
        let Value::Text(ref name) = row[1] else {
            return Err(Error::ParseFailed("[parse_dao_record] Name parsing failed"))
        };
        let name = name.clone();

        let Value::Blob(ref params_bytes) = row[2] else {
            return Err(Error::ParseFailed("[parse_dao_record] Params bytes parsing failed"))
        };
        let params = deserialize_async(params_bytes).await?;

        let leaf_position = match row[3] {
            Value::Blob(ref leaf_position_bytes) => {
                Some(deserialize_async(leaf_position_bytes).await?)
            }
            Value::Null => None,
            _ => {
                return Err(Error::ParseFailed(
                    "[parse_dao_record] Leaf position bytes parsing failed",
                ))
            }
        };

        let tx_hash = match row[4] {
            Value::Blob(ref tx_hash_bytes) => Some(deserialize_async(tx_hash_bytes).await?),
            Value::Null => None,
            _ => {
                return Err(Error::ParseFailed(
                    "[parse_dao_record] Transaction hash bytes parsing failed",
                ))
            }
        };

        let call_index = match row[5] {
            Value::Integer(call_index) => {
                let Ok(call_index) = u8::try_from(call_index) else {
                    return Err(Error::ParseFailed("[parse_dao_record] Call index parsing failed"))
                };
                Some(call_index)
            }
            Value::Null => None,
            _ => return Err(Error::ParseFailed("[parse_dao_record] Call index parsing failed")),
        };

        let dao = DaoRecord::new(name, params, leaf_position, tx_hash, call_index);

        Ok(dao)
    }

    /// Fetch all known DAOs from the wallet.
    pub async fn get_daos(&self) -> Result<Vec<DaoRecord>> {
        let rows = match self.wallet.query_multiple(&DAO_DAOS_TABLE, &[], &[]) {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::RusqliteError(format!("[get_daos] DAOs retrieval failed: {e:?}")))
            }
        };

        let mut daos = Vec::with_capacity(rows.len());
        for row in rows {
            daos.push(self.parse_dao_record(&row).await?);
        }

        Ok(daos)
    }

    /// Auxiliary function to parse a proposal record row.
    async fn parse_dao_proposal(&self, row: &[Value]) -> Result<ProposalRecord> {
        let Value::Blob(ref proposal_bytes) = row[2] else {
            return Err(Error::ParseFailed(
                "[get_dao_proposals] Proposal bytes bytes parsing failed",
            ))
        };
        let proposal = deserialize_async(proposal_bytes).await?;

        let data = match row[3] {
            Value::Blob(ref data_bytes) => Some(deserialize_async(data_bytes).await?),
            Value::Null => None,
            _ => return Err(Error::ParseFailed("[get_dao_proposals] Data bytes parsing failed")),
        };

        let leaf_position = match row[4] {
            Value::Blob(ref leaf_position_bytes) => {
                Some(deserialize_async(leaf_position_bytes).await?)
            }
            Value::Null => None,
            _ => {
                return Err(Error::ParseFailed(
                    "[get_dao_proposals] Leaf position bytes parsing failed",
                ))
            }
        };

        let money_snapshot_tree = match row[5] {
            Value::Blob(ref money_snapshot_tree_bytes) => {
                Some(deserialize_async(money_snapshot_tree_bytes).await?)
            }
            Value::Null => None,
            _ => {
                return Err(Error::ParseFailed(
                    "[get_dao_proposals] Money snapshot tree bytes parsing failed",
                ))
            }
        };

        let nullifiers_smt_snapshot = match row[6] {
            Value::Blob(ref nullifiers_smt_snapshot_bytes) => {
                Some(deserialize_async(nullifiers_smt_snapshot_bytes).await?)
            }
            Value::Null => None,
            _ => {
                return Err(Error::ParseFailed(
                    "[get_dao_proposals] Nullifiers SMT snapshot bytes parsing failed",
                ))
            }
        };

        let tx_hash = match row[7] {
            Value::Blob(ref tx_hash_bytes) => Some(deserialize_async(tx_hash_bytes).await?),
            Value::Null => None,
            _ => {
                return Err(Error::ParseFailed(
                    "[get_dao_proposals] Transaction hash bytes parsing failed",
                ))
            }
        };

        let call_index = match row[8] {
            Value::Integer(call_index) => {
                let Ok(call_index) = u8::try_from(call_index) else {
                    return Err(Error::ParseFailed("[get_dao_proposals] Call index parsing failed"))
                };
                Some(call_index)
            }
            Value::Null => None,
            _ => return Err(Error::ParseFailed("[get_dao_proposals] Call index parsing failed")),
        };

        let exec_tx_hash = match row[9] {
            Value::Blob(ref exec_tx_hash_bytes) => {
                Some(deserialize_async(exec_tx_hash_bytes).await?)
            }
            Value::Null => None,
            _ => {
                return Err(Error::ParseFailed(
                    "[get_dao_proposals] Execution transaction hash bytes parsing failed",
                ))
            }
        };

        Ok(ProposalRecord {
            proposal,
            data,
            leaf_position,
            money_snapshot_tree,
            nullifiers_smt_snapshot,
            tx_hash,
            call_index,
            exec_tx_hash,
        })
    }

    /// Fetch all known DAO proposals from the wallet given a DAO name.
    pub async fn get_dao_proposals(&self, name: &str) -> Result<Vec<ProposalRecord>> {
        let Ok(dao) = self.get_dao_by_name(name).await else {
            return Err(Error::RusqliteError(format!(
                "[get_dao_proposals] DAO with name {name} not found in wallet"
            )))
        };

        let rows = match self.wallet.query_multiple(
            &DAO_PROPOSALS_TABLE,
            &[],
            convert_named_params! {(DAO_PROPOSALS_COL_DAO_BULLA, serialize_async(&dao.bulla()).await)},
        ) {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::RusqliteError(format!(
                    "[get_dao_proposals] Proposals retrieval failed: {e:?}"
                )))
            }
        };

        let mut proposals = Vec::with_capacity(rows.len());
        for row in rows {
            let proposal = self.parse_dao_proposal(&row).await?;
            proposals.push(proposal);
        }

        Ok(proposals)
    }

    /// Append data related to DAO contract transactions into the wallet database.
    pub async fn apply_tx_dao_data(
        &self,
        data: &[u8],
        tx_hash: TransactionHash,
        call_idx: u8,
    ) -> Result<()> {
        // DAOs that have been minted
        let mut new_dao_bullas: Vec<(DaoBulla, TransactionHash, u8)> = vec![];
        // DAO proposals that have been minted
        let mut new_dao_proposals: Vec<ParsedProposal> = vec![];
        // DAO votes that have been seen
        let mut new_dao_votes: Vec<(DaoVoteParams, TransactionHash, u8)> = vec![];

        // We need to clone the trees here for reproducing the snapshot Merkle roots
        let money_tree = self.get_money_tree().await?;
        let nullifiers_smt = self.get_nullifiers_smt().await?;

        // Run through the transaction and see what we got:
        match DaoFunction::try_from(data[0])? {
            DaoFunction::Mint => {
                println!("[apply_tx_dao_data] Found Dao::Mint call");
                let params: DaoMintParams = deserialize_async(&data[1..]).await?;
                new_dao_bullas.push((params.dao_bulla, tx_hash, call_idx));
            }
            DaoFunction::Propose => {
                println!("[apply_tx_dao_data] Found Dao::Propose call");
                let params: DaoProposeParams = deserialize_async(&data[1..]).await?;
                new_dao_proposals.push(ParsedProposal {
                    params,
                    money_tree: money_tree.clone(),
                    nullifiers_smt: nullifiers_smt.clone(),
                    tx_hash,
                    call_idx,
                });
            }
            DaoFunction::Vote => {
                println!("[apply_tx_dao_data] Found Dao::Vote call");
                let params: DaoVoteParams = deserialize_async(&data[1..]).await?;
                new_dao_votes.push((params, tx_hash, call_idx));
            }
            DaoFunction::Exec => {
                println!("[apply_tx_dao_data] Found Dao::Exec call");
                // TODO: implement
            }
            DaoFunction::AuthMoneyTransfer => {
                println!("[apply_tx_dao_data] Found Dao::AuthMoneyTransfer call");
                // Does nothing, just verifies the other calls are correct
            }
        }

        let daos = self.get_daos().await?;
        let mut daos_to_confirm = vec![];
        let (mut daos_tree, mut proposals_tree) = self.get_dao_trees().await?;
        for new_bulla in new_dao_bullas {
            daos_tree.append(MerkleNode::from(new_bulla.0.inner()));
            for dao in &daos {
                if dao.bulla() == new_bulla.0 {
                    println!(
                        "[apply_tx_dao_data] Found minted DAO {}, noting down for wallet update",
                        new_bulla.0
                    );
                    // We have this DAO imported in our wallet. Add the metadata:
                    let mut dao_to_confirm = dao.clone();
                    dao_to_confirm.leaf_position = daos_tree.mark();
                    dao_to_confirm.tx_hash = Some(new_bulla.1);
                    dao_to_confirm.call_index = Some(new_bulla.2);
                    daos_to_confirm.push(dao_to_confirm);
                }
            }
        }

        let mut our_proposals: Vec<ProposalRecord> = vec![];
        for proposal in new_dao_proposals {
            proposals_tree.append(MerkleNode::from(proposal.params.proposal_bulla.inner()));

            // If we're able to decrypt this note, that's the way to link it
            // to a specific DAO.
            for dao in &daos {
                if let Ok(note) =
                    proposal.params.note.decrypt::<DaoProposal>(&dao.params.secret_key)
                {
                    // We managed to decrypt it. Let's place this in a proper ProposalRecord object
                    println!("[apply_tx_dao_data] Managed to decrypt DAO proposal note");

                    // Check if we already got the record
                    let our_proposal = match self
                        .get_dao_proposal_by_bulla(&proposal.params.proposal_bulla)
                        .await
                    {
                        Ok(p) => {
                            let mut our_proposal = p;
                            our_proposal.leaf_position = proposals_tree.mark();
                            our_proposal.money_snapshot_tree = Some(proposal.money_tree);
                            our_proposal.nullifiers_smt_snapshot = Some(proposal.nullifiers_smt);
                            our_proposal.tx_hash = Some(proposal.tx_hash);
                            our_proposal.call_index = Some(proposal.call_idx);
                            our_proposal
                        }
                        Err(_) => ProposalRecord {
                            proposal: note,
                            data: None,
                            leaf_position: proposals_tree.mark(),
                            money_snapshot_tree: Some(proposal.money_tree),
                            nullifiers_smt_snapshot: Some(proposal.nullifiers_smt),
                            tx_hash: Some(proposal.tx_hash),
                            call_index: Some(proposal.call_idx),
                            exec_tx_hash: None,
                        },
                    };

                    our_proposals.push(our_proposal);
                    break
                }
            }
        }

        let mut dao_votes: Vec<VoteRecord> = vec![];
        for vote in new_dao_votes {
            // Check if we got the corresponding proposal
            let mut proposal = None;
            match self.get_dao_proposal_by_bulla(&vote.0.proposal_bulla).await {
                Ok(p) => proposal = Some(p),
                Err(_) => {
                    for p in &our_proposals {
                        if p.bulla() == vote.0.proposal_bulla {
                            proposal = Some(p.clone());
                            break
                        }
                    }
                }
            };
            let Some(proposal) = proposal else { continue };

            // Grab the proposal DAO
            let dao = match self.get_dao_by_bulla(&proposal.proposal.dao_bulla).await {
                Ok(d) => d,
                Err(e) => {
                    println!(
                        "[apply_tx_dao_data] Couldn't find proposal {} DAO {}: {e}",
                        proposal.bulla(),
                        proposal.proposal.dao_bulla,
                    );
                    continue
                }
            };

            // Decrypt the vote note
            let note = match vote.0.note.decrypt_unsafe(&dao.params.secret_key) {
                Ok(n) => n,
                Err(e) => {
                    println!("[apply_tx_dao_data] Couldn't decrypt proposal {} vote with DAO {} keys: {e}",
                        proposal.bulla(),
                        proposal.proposal.dao_bulla,
                    );
                    continue
                }
            };

            // Create the DAO vote record
            let vote_option = fp_to_u64(note[0]).unwrap();
            if vote_option > 1 {
                println!(
                    "[apply_tx_dao_data] Malformed vote for proposal {}: {vote_option}",
                    proposal.bulla(),
                );
                continue
            }
            let vote_option = vote_option != 0;
            let yes_vote_blind = Blind(fp_mod_fv(note[1]));
            let all_vote_value = fp_to_u64(note[2]).unwrap();
            let all_vote_blind = Blind(fp_mod_fv(note[3]));

            let v = VoteRecord {
                id: 0,
                proposal: vote.0.proposal_bulla,
                vote_option,
                yes_vote_blind,
                all_vote_value,
                all_vote_blind,
                tx_hash: vote.1,
                call_index: vote.2,
            };

            dao_votes.push(v);
        }

        if let Err(e) = self.put_dao_trees(&daos_tree, &proposals_tree).await {
            return Err(Error::RusqliteError(format!(
                "[apply_tx_dao_data] Put DAO tree failed: {e:?}"
            )))
        }
        if let Err(e) = self.confirm_daos(&daos_to_confirm).await {
            return Err(Error::RusqliteError(format!(
                "[apply_tx_dao_data] Confirm DAOs failed: {e:?}"
            )))
        }
        if let Err(e) = self.put_dao_proposals(&our_proposals).await {
            return Err(Error::RusqliteError(format!(
                "[apply_tx_dao_data] Put DAO proposals failed: {e:?}"
            )))
        }
        if let Err(e) = self.put_dao_votes(&dao_votes).await {
            return Err(Error::RusqliteError(format!(
                "[apply_tx_dao_data] Put DAO votes failed: {e:?}"
            )))
        }

        Ok(())
    }

    /// Confirm already imported DAO metadata into the wallet.
    /// Here we just write the leaf position, tx hash, and call index.
    /// Panics if the fields are None.
    pub async fn confirm_daos(&self, daos: &[DaoRecord]) -> WalletDbResult<()> {
        for dao in daos {
            let query = format!(
                "UPDATE {} SET {} = ?1, {} = ?2, {} = ?3 WHERE {} = ?4;",
                *DAO_DAOS_TABLE,
                DAO_DAOS_COL_LEAF_POSITION,
                DAO_DAOS_COL_TX_HASH,
                DAO_DAOS_COL_CALL_INDEX,
                DAO_DAOS_COL_BULLA
            );
            self.wallet.exec_sql(
                &query,
                rusqlite::params![
                    serialize_async(&dao.leaf_position.unwrap()).await,
                    serialize_async(&dao.tx_hash.unwrap()).await,
                    dao.call_index.unwrap(),
                    serialize_async(&dao.bulla()).await,
                ],
            )?;
        }

        Ok(())
    }

    /// Unconfirm imported DAOs by removing the leaf position, tx hash, and call index.
    pub async fn unconfirm_daos(&self, daos: &[DaoRecord]) -> WalletDbResult<()> {
        for dao in daos {
            let query = format!(
                "UPDATE {} SET {} = ?1, {} = ?2, {} = ?3 WHERE {} = ?4;",
                *DAO_DAOS_TABLE,
                DAO_DAOS_COL_LEAF_POSITION,
                DAO_DAOS_COL_TX_HASH,
                DAO_DAOS_COL_CALL_INDEX,
                DAO_DAOS_COL_BULLA
            );
            self.wallet.exec_sql(
                &query,
                rusqlite::params![
                    None::<Vec<u8>>,
                    None::<Vec<u8>>,
                    None::<u64>,
                    serialize_async(&dao.bulla()).await
                ],
            )?;
        }

        Ok(())
    }

    /// Import given DAO proposals into the wallet.
    pub async fn put_dao_proposals(&self, proposals: &[ProposalRecord]) -> Result<()> {
        for proposal in proposals {
            if let Err(e) = self.get_dao_by_bulla(&proposal.proposal.dao_bulla).await {
                return Err(Error::RusqliteError(format!(
                    "[put_dao_proposals] Couldn't find proposal {} DAO {}: {e}",
                    proposal.bulla(),
                    proposal.proposal.dao_bulla
                )))
            }

            let query = format!(
                "INSERT OR REPLACE INTO {} ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10);",
                *DAO_PROPOSALS_TABLE,
                DAO_PROPOSALS_COL_BULLA,
                DAO_PROPOSALS_COL_DAO_BULLA,
                DAO_PROPOSALS_COL_PROPOSAL,
                DAO_PROPOSALS_COL_DATA,
                DAO_PROPOSALS_COL_LEAF_POSITION,
                DAO_PROPOSALS_COL_MONEY_SNAPSHOT_TREE,
                DAO_PROPOSALS_COL_NULLIFIERS_SMT_SNAPSHOT,
                DAO_PROPOSALS_COL_TX_HASH,
                DAO_PROPOSALS_COL_CALL_INDEX,
                DAO_PROPOSALS_COL_EXEC_TX_HASH,
            );

            let data = match &proposal.data {
                Some(data) => Some(serialize_async(data).await),
                None => None,
            };

            let leaf_position = match &proposal.leaf_position {
                Some(leaf_position) => Some(serialize_async(leaf_position).await),
                None => None,
            };

            let money_snapshot_tree = match &proposal.money_snapshot_tree {
                Some(money_snapshot_tree) => Some(serialize_async(money_snapshot_tree).await),
                None => None,
            };

            let nullifiers_smt_snapshot = match &proposal.nullifiers_smt_snapshot {
                Some(nullifiers_smt_snapshot) => {
                    Some(serialize_async(nullifiers_smt_snapshot).await)
                }
                None => None,
            };

            let tx_hash = match &proposal.tx_hash {
                Some(tx_hash) => Some(serialize_async(tx_hash).await),
                None => None,
            };

            let exec_tx_hash = match &proposal.exec_tx_hash {
                Some(exec_tx_hash) => Some(serialize_async(exec_tx_hash).await),
                None => None,
            };

            if let Err(e) = self.wallet.exec_sql(
                &query,
                rusqlite::params![
                    serialize_async(&proposal.bulla()).await,
                    serialize_async(&proposal.proposal.dao_bulla).await,
                    serialize_async(&proposal.proposal).await,
                    data,
                    leaf_position,
                    money_snapshot_tree,
                    nullifiers_smt_snapshot,
                    tx_hash,
                    proposal.call_index,
                    exec_tx_hash,
                ],
            ) {
                return Err(Error::RusqliteError(format!(
                    "[put_dao_proposals] Proposal insert failed: {e:?}"
                )))
            };
        }

        Ok(())
    }

    /// Unconfirm imported DAO proposals by removing the leaf position, tx hash, and call index.
    pub async fn unconfirm_proposals(&self, proposals: &[ProposalRecord]) -> WalletDbResult<()> {
        for proposal in proposals {
            let query = format!(
                "UPDATE {} SET {} = ?1, {} = ?2, {} = ?3, {} = ?4, {} = ?5, {} = ?6 WHERE {} = ?7;",
                *DAO_PROPOSALS_TABLE,
                DAO_PROPOSALS_COL_LEAF_POSITION,
                DAO_PROPOSALS_COL_MONEY_SNAPSHOT_TREE,
                DAO_PROPOSALS_COL_NULLIFIERS_SMT_SNAPSHOT,
                DAO_PROPOSALS_COL_TX_HASH,
                DAO_PROPOSALS_COL_CALL_INDEX,
                DAO_PROPOSALS_COL_EXEC_TX_HASH,
                DAO_PROPOSALS_COL_BULLA
            );
            self.wallet.exec_sql(
                &query,
                rusqlite::params![
                    None::<Vec<u8>>,
                    None::<Vec<u8>>,
                    None::<Vec<u8>>,
                    None::<Vec<u8>>,
                    None::<u64>,
                    None::<Vec<u8>>,
                    serialize_async(&proposal.bulla()).await
                ],
            )?;
        }

        Ok(())
    }

    /// Import given DAO votes into the wallet.
    pub async fn put_dao_votes(&self, votes: &[VoteRecord]) -> WalletDbResult<()> {
        for vote in votes {
            eprintln!("Importing DAO vote into wallet");

            let query = format!(
                "INSERT INTO {} ({}, {}, {}, {}, {}, {}, {}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7);",
                *DAO_VOTES_TABLE,
                DAO_VOTES_COL_PROPOSAL_BULLA,
                DAO_VOTES_COL_VOTE_OPTION,
                DAO_VOTES_COL_YES_VOTE_BLIND,
                DAO_VOTES_COL_ALL_VOTE_VALUE,
                DAO_VOTES_COL_ALL_VOTE_BLIND,
                DAO_VOTES_COL_TX_HASH,
                DAO_VOTES_COL_CALL_INDEX,
            );

            self.wallet.exec_sql(
                &query,
                rusqlite::params![
                    serialize_async(&vote.proposal).await,
                    vote.vote_option as u64,
                    serialize_async(&vote.yes_vote_blind).await,
                    serialize_async(&vote.all_vote_value).await,
                    serialize_async(&vote.all_vote_blind).await,
                    serialize_async(&vote.tx_hash).await,
                    vote.call_index,
                ],
            )?;

            println!("DAO vote added to wallet");
        }

        Ok(())
    }

    /// Reset the DAO Merkle trees in the wallet.
    pub async fn reset_dao_trees(&self) -> WalletDbResult<()> {
        println!("Resetting DAO Merkle trees");
        let tree = MerkleTree::new(1);
        self.put_dao_trees(&tree, &tree).await?;
        println!("Successfully reset DAO Merkle trees");

        Ok(())
    }

    /// Reset confirmed DAOs in the wallet.
    pub async fn reset_daos(&self) -> WalletDbResult<()> {
        println!("Resetting DAO confirmations");
        let daos = match self.get_daos().await {
            Ok(d) => d,
            Err(e) => {
                println!("[reset_daos] DAOs retrieval failed: {e:?}");
                return Err(WalletDbError::GenericError);
            }
        };
        self.unconfirm_daos(&daos).await?;
        println!("Successfully unconfirmed DAOs");

        Ok(())
    }

    /// Reset all DAO proposals in the wallet.
    pub async fn reset_dao_proposals(&self) -> WalletDbResult<()> {
        println!("Resetting DAO proposals confirmations");
        let proposals = match self.get_proposals().await {
            Ok(p) => p,
            Err(e) => {
                println!("[reset_dao_proposals] DAO proposals retrieval failed: {e:?}");
                return Err(WalletDbError::GenericError);
            }
        };
        self.unconfirm_proposals(&proposals).await?;
        println!("Successfully unconfirmed DAO proposals");

        Ok(())
    }

    /// Reset all DAO votes in the wallet.
    pub fn reset_dao_votes(&self) -> WalletDbResult<()> {
        println!("Resetting DAO votes");
        let query = format!("DELETE FROM {};", *DAO_VOTES_TABLE);
        self.wallet.exec_sql(&query, &[])
    }

    /// Import given DAO params into the wallet with a given name.
    pub async fn import_dao(&self, name: &str, params: DaoParams) -> Result<()> {
        // First let's check if we've imported this DAO with the given name before.
        if self.get_dao_by_name(name).await.is_ok() {
            return Err(Error::RusqliteError(
                "[import_dao] This DAO has already been imported".to_string(),
            ))
        }

        println!("Importing \"{name}\" DAO into the wallet");

        let query = format!(
            "INSERT INTO {} ({}, {}, {}) VALUES (?1, ?2, ?3);",
            *DAO_DAOS_TABLE, DAO_DAOS_COL_BULLA, DAO_DAOS_COL_NAME, DAO_DAOS_COL_PARAMS,
        );
        if let Err(e) = self.wallet.exec_sql(
            &query,
            rusqlite::params![
                serialize_async(&params.dao.to_bulla()).await,
                name,
                serialize_async(&params).await,
            ],
        ) {
            return Err(Error::RusqliteError(format!("[import_dao] DAO insert failed: {e:?}")))
        };

        Ok(())
    }

    /// Fetch a DAO given its bulla.
    pub async fn get_dao_by_bulla(&self, bulla: &DaoBulla) -> Result<DaoRecord> {
        let row = match self.wallet.query_single(
            &DAO_DAOS_TABLE,
            &[],
            convert_named_params! {(DAO_DAOS_COL_BULLA, serialize_async(bulla).await)},
        ) {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::RusqliteError(format!(
                    "[get_dao_by_bulla] DAO retrieval failed: {e:?}"
                )))
            }
        };

        self.parse_dao_record(&row).await
    }

    /// Fetch a DAO given its name.
    pub async fn get_dao_by_name(&self, name: &str) -> Result<DaoRecord> {
        let row = match self.wallet.query_single(
            &DAO_DAOS_TABLE,
            &[],
            convert_named_params! {(DAO_DAOS_COL_NAME, name)},
        ) {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::RusqliteError(format!(
                    "[get_dao_by_name] DAO retrieval failed: {e:?}"
                )))
            }
        };

        self.parse_dao_record(&row).await
    }

    /// List DAO(s) imported in the wallet. If a name is given, just print the
    /// metadata for that specific one, if found.
    pub async fn dao_list(&self, name: &Option<String>) -> Result<()> {
        if let Some(name) = name {
            let dao = self.get_dao_by_name(name).await?;
            println!("{dao}");
            return Ok(());
        }

        let daos = self.get_daos().await?;
        for (i, dao) in daos.iter().enumerate() {
            println!("{i}. {}", dao.name);
        }

        Ok(())
    }

    /// Fetch known unspent balances from the wallet for the given DAO name.
    pub async fn dao_balance(&self, name: &str) -> Result<HashMap<String, u64>> {
        let dao = self.get_dao_by_name(name).await?;

        let dao_spend_hook =
            FuncRef { contract_id: *DAO_CONTRACT_ID, func_code: DaoFunction::Exec as u8 }
                .to_func_id();

        let mut coins = self.get_coins(false).await?;
        coins.retain(|x| x.0.note.spend_hook == dao_spend_hook);
        coins.retain(|x| x.0.note.user_data == dao.bulla().inner());

        // Fill this map with balances
        let mut balmap: HashMap<String, u64> = HashMap::new();

        for coin in coins {
            let mut value = coin.0.note.value;

            if let Some(prev) = balmap.get(&coin.0.note.token_id.to_string()) {
                value += prev;
            }

            balmap.insert(coin.0.note.token_id.to_string(), value);
        }

        Ok(balmap)
    }

    /// Fetch all known DAO proposalss from the wallet.
    pub async fn get_proposals(&self) -> Result<Vec<ProposalRecord>> {
        let rows = match self.wallet.query_multiple(&DAO_PROPOSALS_TABLE, &[], &[]) {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::RusqliteError(format!(
                    "[get_proposals] DAO proposalss retrieval failed: {e:?}"
                )))
            }
        };

        let mut daos = Vec::with_capacity(rows.len());
        for row in rows {
            daos.push(self.parse_dao_proposal(&row).await?);
        }

        Ok(daos)
    }

    /// Fetch a DAO proposal by its bulla.
    pub async fn get_dao_proposal_by_bulla(
        &self,
        bulla: &DaoProposalBulla,
    ) -> Result<ProposalRecord> {
        // Grab the proposal record
        let row = match self.wallet.query_single(
            &DAO_PROPOSALS_TABLE,
            &[],
            convert_named_params! {(DAO_PROPOSALS_COL_BULLA, serialize_async(bulla).await)},
        ) {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::RusqliteError(format!(
                    "[get_dao_proposal_by_bulla] DAO proposal retrieval failed: {e:?}"
                )))
            }
        };

        // Parse rest of the record
        self.parse_dao_proposal(&row).await
    }

    // Fetch all known DAO proposal votes from the wallet given a proposal ID.
    pub async fn get_dao_proposal_votes(
        &self,
        proposal: &DaoProposalBulla,
    ) -> Result<Vec<VoteRecord>> {
        let rows = match self.wallet.query_multiple(
            &DAO_VOTES_TABLE,
            &[],
            convert_named_params! {(DAO_VOTES_COL_PROPOSAL_BULLA, serialize_async(proposal).await)},
        ) {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::RusqliteError(format!(
                    "[get_dao_proposal_votes] Votes retrieval failed: {e:?}"
                )))
            }
        };

        let mut votes = Vec::with_capacity(rows.len());
        for row in rows {
            let Value::Integer(id) = row[0] else {
                return Err(Error::ParseFailed("[get_dao_proposal_votes] ID parsing failed"))
            };
            let Ok(id) = u64::try_from(id) else {
                return Err(Error::ParseFailed("[get_dao_proposal_votes] ID parsing failed"))
            };

            let Value::Blob(ref proposal_bytes) = row[1] else {
                return Err(Error::ParseFailed(
                    "[get_dao_proposal_votes] Proposal bytes bytes parsing failed",
                ))
            };
            let proposal = deserialize_async(proposal_bytes).await?;

            let Value::Integer(vote_option) = row[2] else {
                return Err(Error::ParseFailed(
                    "[get_dao_proposal_votes] Vote option parsing failed",
                ))
            };
            let Ok(vote_option) = u32::try_from(vote_option) else {
                return Err(Error::ParseFailed(
                    "[get_dao_proposal_votes] Vote option parsing failed",
                ))
            };
            let vote_option = vote_option != 0;

            let Value::Blob(ref yes_vote_blind_bytes) = row[3] else {
                return Err(Error::ParseFailed(
                    "[get_dao_proposal_votes] Yes vote blind bytes parsing failed",
                ))
            };
            let yes_vote_blind = deserialize_async(yes_vote_blind_bytes).await?;

            let Value::Blob(ref all_vote_value_bytes) = row[4] else {
                return Err(Error::ParseFailed(
                    "[get_dao_proposal_votes] All vote value bytes parsing failed",
                ))
            };
            let all_vote_value = deserialize_async(all_vote_value_bytes).await?;

            let Value::Blob(ref all_vote_blind_bytes) = row[5] else {
                return Err(Error::ParseFailed(
                    "[get_dao_proposal_votes] All vote blind bytes parsing failed",
                ))
            };
            let all_vote_blind = deserialize_async(all_vote_blind_bytes).await?;

            let Value::Blob(ref tx_hash_bytes) = row[6] else {
                return Err(Error::ParseFailed(
                    "[get_dao_proposal_votes] Transaction hash bytes parsing failed",
                ))
            };
            let tx_hash = deserialize_async(tx_hash_bytes).await?;

            let Value::Integer(call_index) = row[7] else {
                return Err(Error::ParseFailed("[get_dao_proposal_votes] Call index parsing failed"))
            };
            let Ok(call_index) = u8::try_from(call_index) else {
                return Err(Error::ParseFailed("[get_dao_proposal_votes] Call index parsing failed"))
            };

            let vote = VoteRecord {
                id,
                proposal,
                vote_option,
                yes_vote_blind,
                all_vote_value,
                all_vote_blind,
                tx_hash,
                call_index,
            };

            votes.push(vote);
        }

        Ok(votes)
    }

    /// Mint a DAO on-chain.
    pub async fn dao_mint(&self, name: &str) -> Result<Transaction> {
        // Retrieve the dao record
        let dao = self.get_dao_by_name(name).await?;

        // Check its not already minted
        if dao.tx_hash.is_some() {
            return Err(Error::Custom(
                "[dao_mint] This DAO seems to have already been minted on-chain".to_string(),
            ))
        }

        // Now we need to do a lookup for the zkas proof bincodes, and create
        // the circuit objects and proving keys so we can build the transaction.
        // We also do this through the RPC. First we grab the fee call from money.
        let zkas_bins = self.lookup_zkas(&MONEY_CONTRACT_ID).await?;

        let Some(fee_zkbin) = zkas_bins.iter().find(|x| x.0 == MONEY_CONTRACT_ZKAS_FEE_NS_V1)
        else {
            return Err(Error::Custom("Fee circuit not found".to_string()))
        };

        let fee_zkbin = ZkBinary::decode(&fee_zkbin.1)?;

        let fee_circuit = ZkCircuit::new(empty_witnesses(&fee_zkbin)?, &fee_zkbin);

        // Creating Fee circuit proving key
        let fee_pk = ProvingKey::build(fee_zkbin.k, &fee_circuit);

        // Now we grab the DAO mint
        let zkas_bins = self.lookup_zkas(&DAO_CONTRACT_ID).await?;

        let Some(dao_mint_zkbin) = zkas_bins.iter().find(|x| x.0 == DAO_CONTRACT_ZKAS_DAO_MINT_NS)
        else {
            return Err(Error::RusqliteError("[dao_mint] DAO Mint circuit not found".to_string()))
        };

        let dao_mint_zkbin = ZkBinary::decode(&dao_mint_zkbin.1)?;

        let dao_mint_circuit = ZkCircuit::new(empty_witnesses(&dao_mint_zkbin)?, &dao_mint_zkbin);

        // Creating DAO Mint circuit proving key
        let dao_mint_pk = ProvingKey::build(dao_mint_zkbin.k, &dao_mint_circuit);

        // Create the DAO mint call
        let (params, proofs) =
            make_mint_call(&dao.params.dao, &dao.params.secret_key, &dao_mint_zkbin, &dao_mint_pk)?;
        let mut data = vec![DaoFunction::Mint as u8];
        params.encode_async(&mut data).await?;
        let call = ContractCall { contract_id: *DAO_CONTRACT_ID, data };

        // Create the TransactionBuilder containing above call
        let mut tx_builder = TransactionBuilder::new(ContractCallLeaf { call, proofs }, vec![])?;

        // We first have to execute the fee-less tx to gather its used gas, and then we feed
        // it into the fee-creating function.
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&[dao.params.secret_key])?;
        tx.signatures.push(sigs);

        let tree = self.get_money_tree().await?;
        let (fee_call, fee_proofs, fee_secrets) =
            self.append_fee_call(&tx, &tree, &fee_pk, &fee_zkbin, None).await?;

        // Append the fee call to the transaction
        tx_builder.append(ContractCallLeaf { call: fee_call, proofs: fee_proofs }, vec![])?;

        // Now build the actual transaction and sign it with all necessary keys.
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&[dao.params.secret_key])?;
        tx.signatures.push(sigs);
        let sigs = tx.create_sigs(&fee_secrets)?;
        tx.signatures.push(sigs);

        Ok(tx)
    }

    /// Create a DAO transfer proposal.
    #[allow(clippy::too_many_arguments)]
    pub async fn dao_propose_transfer(
        &self,
        name: &str,
        duration_days: u64,
        amount: &str,
        token_id: TokenId,
        recipient: PublicKey,
        spend_hook: Option<FuncId>,
        user_data: Option<pallas::Base>,
    ) -> Result<ProposalRecord> {
        // Fetch DAO and check its deployed
        let dao = self.get_dao_by_name(name).await?;
        if dao.leaf_position.is_none() || dao.tx_hash.is_none() || dao.call_index.is_none() {
            return Err(Error::Custom(
                "[dao_propose_transfer] DAO seems to not have been deployed yet".to_string(),
            ))
        }

        // Fetch DAO unspent OwnCoins to see what its balance is
        let dao_spend_hook =
            FuncRef { contract_id: *DAO_CONTRACT_ID, func_code: DaoFunction::Exec as u8 }
                .to_func_id();
        let dao_bulla = dao.bulla();
        let dao_owncoins =
            self.get_contract_token_coins(&token_id, &dao_spend_hook, &dao_bulla.inner()).await?;
        if dao_owncoins.is_empty() {
            return Err(Error::Custom(format!(
                "[dao_propose_transfer] Did not find any {token_id} unspent coins owned by this DAO"
            )))
        }

        // Check DAO balance is sufficient
        let amount = decode_base10(amount, BALANCE_BASE10_DECIMALS, false)?;
        if dao_owncoins.iter().map(|x| x.note.value).sum::<u64>() < amount {
            return Err(Error::Custom(format!(
                "[dao_propose_transfer] Not enough DAO balance for token ID: {token_id}",
            )))
        }

        // Generate proposal coin attributes
        let proposal_coinattrs = vec![CoinAttributes {
            public_key: recipient,
            value: amount,
            token_id,
            spend_hook: spend_hook.unwrap_or(FuncId::none()),
            user_data: user_data.unwrap_or(pallas::Base::ZERO),
            blind: Blind::random(&mut OsRng),
        }];

        // Convert coin_params to actual coins
        let mut proposal_coins = vec![];
        for coin_params in &proposal_coinattrs {
            proposal_coins.push(coin_params.to_coin());
        }
        let mut proposal_data = vec![];
        proposal_coins.encode_async(&mut proposal_data).await?;

        // Create Auth calls
        let auth_calls = vec![
            DaoAuthCall {
                contract_id: *DAO_CONTRACT_ID,
                function_code: DaoFunction::AuthMoneyTransfer as u8,
                auth_data: proposal_data,
            },
            DaoAuthCall {
                contract_id: *MONEY_CONTRACT_ID,
                function_code: MoneyFunction::TransferV1 as u8,
                auth_data: vec![],
            },
        ];

        // Retrieve current block height and compute current day
        let current_block_height = self.get_next_block_height().await?;
        let creation_day = blockwindow(current_block_height);

        // Create the actual proposal
        let proposal = DaoProposal {
            auth_calls,
            creation_day,
            duration_days,
            user_data: user_data.unwrap_or(pallas::Base::ZERO),
            dao_bulla,
            blind: Blind::random(&mut OsRng),
        };

        let proposal_record = ProposalRecord {
            proposal,
            data: Some(serialize_async(&proposal_coinattrs).await),
            leaf_position: None,
            money_snapshot_tree: None,
            nullifiers_smt_snapshot: None,
            tx_hash: None,
            call_index: None,
            exec_tx_hash: None,
        };

        if let Err(e) = self.put_dao_proposals(&[proposal_record.clone()]).await {
            return Err(Error::RusqliteError(format!(
                "[dao_propose_transfer] Put DAO proposals failed: {e:?}"
            )))
        }

        Ok(proposal_record)
    }

    /// Create a DAO transfer proposal transaction.
    pub async fn dao_transfer_proposal_tx(&self, proposal: &ProposalRecord) -> Result<Transaction> {
        // Check we know the plaintext data
        if proposal.data.is_none() {
            return Err(Error::Custom(
                "[dao_transfer_proposal_tx] Proposal plainext data is empty".to_string(),
            ))
        }
        let proposal_coinattrs: Vec<CoinAttributes> =
            deserialize_async(proposal.data.as_ref().unwrap()).await?;

        // Fetch DAO and check its deployed
        let Ok(dao) = self.get_dao_by_bulla(&proposal.proposal.dao_bulla).await else {
            return Err(Error::Custom(format!(
                "[dao_transfer_proposal_tx] DAO {} was not found",
                proposal.proposal.dao_bulla
            )))
        };
        if dao.leaf_position.is_none() || dao.tx_hash.is_none() || dao.call_index.is_none() {
            return Err(Error::Custom(
                "[dao_transfer_proposal_tx] DAO seems to not have been deployed yet".to_string(),
            ))
        }

        // Fetch DAO unspent OwnCoins to see what its balance is for each coin
        let dao_spend_hook =
            FuncRef { contract_id: *DAO_CONTRACT_ID, func_code: DaoFunction::Exec as u8 }
                .to_func_id();
        for coinattr in proposal_coinattrs {
            let dao_owncoins = self
                .get_contract_token_coins(
                    &coinattr.token_id,
                    &dao_spend_hook,
                    &proposal.proposal.dao_bulla.inner(),
                )
                .await?;
            if dao_owncoins.is_empty() {
                return Err(Error::Custom(format!(
                    "[dao_transfer_proposal_tx] Did not find any {} unspent coins owned by this DAO",
                    coinattr.token_id,
                )))
            }

            // Check DAO balance is sufficient
            if dao_owncoins.iter().map(|x| x.note.value).sum::<u64>() < coinattr.value {
                return Err(Error::Custom(format!(
                    "[dao_transfer_proposal_tx] Not enough DAO balance for token ID: {}",
                    coinattr.token_id,
                )))
            }
        }

        // Fetch our own governance OwnCoins to see what our balance is
        let gov_owncoins = self.get_token_coins(&dao.params.dao.gov_token_id).await?;
        if gov_owncoins.is_empty() {
            return Err(Error::Custom(format!(
                "[dao_transfer_proposal_tx] Did not find any governance {} coins in wallet",
                dao.params.dao.gov_token_id
            )))
        }

        // Find which governance coins we can use
        let mut total_value = 0;
        let mut gov_owncoins_to_use = vec![];
        for gov_owncoin in gov_owncoins {
            if total_value >= dao.params.dao.proposer_limit {
                break
            }

            total_value += gov_owncoin.note.value;
            gov_owncoins_to_use.push(gov_owncoin);
        }

        // Check our governance coins balance is sufficient
        if total_value < dao.params.dao.proposer_limit {
            return Err(Error::Custom(format!(
                "[dao_transfer_proposal_tx] Not enough gov token {} balance to propose",
                dao.params.dao.gov_token_id
            )))
        }

        // Now we need to do a lookup for the zkas proof bincodes, and create
        // the circuit objects and proving keys so we can build the transaction.
        // We also do this through the RPC. First we grab the fee call from money.
        let zkas_bins = self.lookup_zkas(&MONEY_CONTRACT_ID).await?;

        let Some(fee_zkbin) = zkas_bins.iter().find(|x| x.0 == MONEY_CONTRACT_ZKAS_FEE_NS_V1)
        else {
            return Err(Error::Custom(
                "[dao_transfer_proposal_tx] Fee circuit not found".to_string(),
            ))
        };

        let fee_zkbin = ZkBinary::decode(&fee_zkbin.1)?;

        let fee_circuit = ZkCircuit::new(empty_witnesses(&fee_zkbin)?, &fee_zkbin);

        // Creating Fee circuit proving key
        let fee_pk = ProvingKey::build(fee_zkbin.k, &fee_circuit);

        // Now we grab the DAO bins
        let zkas_bins = self.lookup_zkas(&DAO_CONTRACT_ID).await?;

        let Some(propose_burn_zkbin) =
            zkas_bins.iter().find(|x| x.0 == DAO_CONTRACT_ZKAS_DAO_PROPOSE_INPUT_NS)
        else {
            return Err(Error::Custom(
                "[dao_transfer_proposal_tx] Propose Burn circuit not found".to_string(),
            ))
        };

        let Some(propose_main_zkbin) =
            zkas_bins.iter().find(|x| x.0 == DAO_CONTRACT_ZKAS_DAO_PROPOSE_MAIN_NS)
        else {
            return Err(Error::Custom(
                "[dao_transfer_proposal_tx] Propose Main circuit not found".to_string(),
            ))
        };

        let propose_burn_zkbin = ZkBinary::decode(&propose_burn_zkbin.1)?;
        let propose_main_zkbin = ZkBinary::decode(&propose_main_zkbin.1)?;

        let propose_burn_circuit =
            ZkCircuit::new(empty_witnesses(&propose_burn_zkbin)?, &propose_burn_zkbin);
        let propose_main_circuit =
            ZkCircuit::new(empty_witnesses(&propose_main_zkbin)?, &propose_main_zkbin);

        // Creating DAO ProposeBurn and ProposeMain circuits proving keys
        let propose_burn_pk = ProvingKey::build(propose_burn_zkbin.k, &propose_burn_circuit);
        let propose_main_pk = ProvingKey::build(propose_main_zkbin.k, &propose_main_circuit);

        // Fetch our money Merkle tree
        let money_merkle_tree = self.get_money_tree().await?;

        // Now we can create the proposal transaction parameters.
        // We first generate the `DaoProposeStakeInput` inputs,
        // using our governance OwnCoins.
        let mut inputs = Vec::with_capacity(gov_owncoins_to_use.len());
        for gov_owncoin in gov_owncoins_to_use {
            let input = DaoProposeStakeInput {
                secret: gov_owncoin.secret,
                note: gov_owncoin.note.clone(),
                leaf_position: gov_owncoin.leaf_position,
                merkle_path: money_merkle_tree.witness(gov_owncoin.leaf_position, 0).unwrap(),
            };
            inputs.push(input);
        }

        // Now create the parameters for the proposal tx
        let signature_secret = SecretKey::random(&mut OsRng);

        // Fetch the daos Merkle tree to compute the DAO Merkle path and root
        let (daos_tree, _) = self.get_dao_trees().await?;
        let (dao_merkle_path, dao_merkle_root) = {
            let root = daos_tree.root(0).unwrap();
            let leaf_pos = dao.leaf_position.unwrap();
            let dao_merkle_path = daos_tree.witness(leaf_pos, 0).unwrap();
            (dao_merkle_path, root)
        };

        // Generate the Money nullifiers Sparse Merkle Tree
        let store = WalletStorage::new(
            &self.wallet,
            &MONEY_SMT_TABLE,
            MONEY_SMT_COL_KEY,
            MONEY_SMT_COL_VALUE,
        );
        let money_null_smt = WalletSmt::new(store, PoseidonFp::new(), &EMPTY_NODES_FP);

        // Create the proposal call
        let call = DaoProposeCall {
            money_null_smt: &money_null_smt,
            inputs,
            proposal: proposal.proposal.clone(),
            dao: dao.params.dao,
            dao_leaf_position: dao.leaf_position.unwrap(),
            dao_merkle_path,
            dao_merkle_root,
            signature_secret,
        };

        let (params, proofs) = call.make(
            &propose_burn_zkbin,
            &propose_burn_pk,
            &propose_main_zkbin,
            &propose_main_pk,
        )?;

        // Encode the call
        let mut data = vec![DaoFunction::Propose as u8];
        params.encode_async(&mut data).await?;
        let call = ContractCall { contract_id: *DAO_CONTRACT_ID, data };

        // Create the TransactionBuilder containing above call
        let mut tx_builder = TransactionBuilder::new(ContractCallLeaf { call, proofs }, vec![])?;

        // We first have to execute the fee-less tx to gather its used gas, and then we feed
        // it into the fee-creating function.
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&[signature_secret])?;
        tx.signatures = vec![sigs];

        let tree = self.get_money_tree().await?;
        let (fee_call, fee_proofs, fee_secrets) =
            self.append_fee_call(&tx, &tree, &fee_pk, &fee_zkbin, None).await?;

        // Append the fee call to the transaction
        tx_builder.append(ContractCallLeaf { call: fee_call, proofs: fee_proofs }, vec![])?;

        // Now build the actual transaction and sign it with all necessary keys.
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&[signature_secret])?;
        tx.signatures.push(sigs);
        let sigs = tx.create_sigs(&fee_secrets)?;
        tx.signatures.push(sigs);

        Ok(tx)
    }

    /// Vote on a DAO proposal
    pub async fn dao_vote(
        &self,
        proposal: &DaoProposalBulla,
        vote_option: bool,
        weight: Option<u64>,
    ) -> Result<Transaction> {
        // Feth the proposal and check its deployed
        let Ok(proposal) = self.get_dao_proposal_by_bulla(proposal).await else {
            return Err(Error::Custom(format!("[dao_vote] Proposal {} was not found", proposal)))
        };
        if proposal.leaf_position.is_none() ||
            proposal.money_snapshot_tree.is_none() ||
            proposal.nullifiers_smt_snapshot.is_none() ||
            proposal.tx_hash.is_none() ||
            proposal.call_index.is_none()
        {
            return Err(Error::Custom(
                "[dao_vote] Proposal seems to not have been deployed yet".to_string(),
            ))
        }

        // Check we know the plaintext data
        if proposal.data.is_none() {
            return Err(Error::Custom("[dao_vote] Proposal plainext data is empty".to_string()))
        }

        // Fetch DAO and check its deployed
        let Ok(dao) = self.get_dao_by_bulla(&proposal.proposal.dao_bulla).await else {
            return Err(Error::Custom(format!(
                "[dao_vote] DAO {} was not found",
                proposal.proposal.dao_bulla
            )))
        };
        if dao.leaf_position.is_none() || dao.tx_hash.is_none() || dao.call_index.is_none() {
            return Err(Error::Custom(
                "[dao_vote] DAO seems to not have been deployed yet".to_string(),
            ))
        }

        // Fetch our own governance OwnCoins to see what our balance is
        let gov_owncoins = self.get_token_coins(&dao.params.dao.gov_token_id).await?;
        if gov_owncoins.is_empty() {
            return Err(Error::Custom(format!(
                "[dao_vote] Did not find any governance {} coins in wallet",
                dao.params.dao.gov_token_id
            )))
        }

        // Find which governance coins we can use
        let gov_owncoins_to_use = match weight {
            Some(_weight) => {
                // TODO: Build a proper coin selection algorithm so that we can use a
                // coins combination that matches the requested weight
                return Err(Error::Custom(
                    "[dao_vote] Fractional vote weight not supported yet".to_string(),
                ))
            }
            // If no weight was specified, use them all
            None => gov_owncoins,
        };

        // Now we need to do a lookup for the zkas proof bincodes, and create
        // the circuit objects and proving keys so we can build the transaction.
        // We also do this through the RPC. First we grab the fee call from money.
        let zkas_bins = self.lookup_zkas(&MONEY_CONTRACT_ID).await?;

        let Some(fee_zkbin) = zkas_bins.iter().find(|x| x.0 == MONEY_CONTRACT_ZKAS_FEE_NS_V1)
        else {
            return Err(Error::Custom("[dao_vote] Fee circuit not found".to_string()))
        };

        let fee_zkbin = ZkBinary::decode(&fee_zkbin.1)?;

        let fee_circuit = ZkCircuit::new(empty_witnesses(&fee_zkbin)?, &fee_zkbin);

        // Creating Fee circuit proving key
        let fee_pk = ProvingKey::build(fee_zkbin.k, &fee_circuit);

        // Now we grab the DAO bins
        let zkas_bins = self.lookup_zkas(&DAO_CONTRACT_ID).await?;

        let Some(dao_vote_burn_zkbin) =
            zkas_bins.iter().find(|x| x.0 == DAO_CONTRACT_ZKAS_DAO_VOTE_INPUT_NS)
        else {
            return Err(Error::Custom("[dao_vote] DAO Vote Burn circuit not found".to_string()))
        };

        let Some(dao_vote_main_zkbin) =
            zkas_bins.iter().find(|x| x.0 == DAO_CONTRACT_ZKAS_DAO_VOTE_MAIN_NS)
        else {
            return Err(Error::Custom("[dao_vote] DAO Vote Main circuit not found".to_string()))
        };

        let dao_vote_burn_zkbin = ZkBinary::decode(&dao_vote_burn_zkbin.1)?;
        let dao_vote_main_zkbin = ZkBinary::decode(&dao_vote_main_zkbin.1)?;

        let dao_vote_burn_circuit =
            ZkCircuit::new(empty_witnesses(&dao_vote_burn_zkbin)?, &dao_vote_burn_zkbin);
        let dao_vote_main_circuit =
            ZkCircuit::new(empty_witnesses(&dao_vote_main_zkbin)?, &dao_vote_main_zkbin);

        // Creating DAO VoteBurn and VoteMain circuits proving keys
        let dao_vote_burn_pk = ProvingKey::build(dao_vote_burn_zkbin.k, &dao_vote_burn_circuit);
        let dao_vote_main_pk = ProvingKey::build(dao_vote_main_zkbin.k, &dao_vote_main_circuit);

        // Now create the parameters for the vote tx
        let signature_secret = SecretKey::random(&mut OsRng);
        let mut inputs = Vec::with_capacity(gov_owncoins_to_use.len());
        for gov_owncoin in gov_owncoins_to_use {
            let input = DaoVoteInput {
                secret: gov_owncoin.secret,
                note: gov_owncoin.note.clone(),
                leaf_position: gov_owncoin.leaf_position,
                merkle_path: proposal
                    .money_snapshot_tree
                    .as_ref()
                    .unwrap()
                    .witness(gov_owncoin.leaf_position, 0)
                    .unwrap(),
                signature_secret,
            };
            inputs.push(input);
        }

        // Retrieve current block height and compute current window
        let current_block_height = self.get_next_block_height().await?;
        let current_day = blockwindow(current_block_height);

        // Generate the Money nullifiers Sparse Merkle Tree
        let store = MemoryStorageFp { tree: proposal.nullifiers_smt_snapshot.unwrap() };
        let money_null_smt = SmtMemoryFp::new(store, PoseidonFp::new(), &EMPTY_NODES_FP);

        // Create the vote call
        let call = DaoVoteCall {
            money_null_smt: &money_null_smt,
            inputs,
            vote_option,
            proposal: proposal.proposal.clone(),
            dao: dao.params.dao.clone(),
            dao_keypair: dao.keypair(),
            current_day,
        };

        let (params, proofs) = call.make(
            &dao_vote_burn_zkbin,
            &dao_vote_burn_pk,
            &dao_vote_main_zkbin,
            &dao_vote_main_pk,
        )?;

        // Encode the call
        let mut data = vec![DaoFunction::Vote as u8];
        params.encode_async(&mut data).await?;
        let call = ContractCall { contract_id: *DAO_CONTRACT_ID, data };

        // Create the TransactionBuilder containing above call
        let mut tx_builder = TransactionBuilder::new(ContractCallLeaf { call, proofs }, vec![])?;

        // We first have to execute the fee-less tx to gather its used gas, and then we feed
        // it into the fee-creating function.
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&[signature_secret])?;
        tx.signatures = vec![sigs];

        let tree = self.get_money_tree().await?;
        let (fee_call, fee_proofs, fee_secrets) =
            self.append_fee_call(&tx, &tree, &fee_pk, &fee_zkbin, None).await?;

        // Append the fee call to the transaction
        tx_builder.append(ContractCallLeaf { call: fee_call, proofs: fee_proofs }, vec![])?;

        // Now build the actual transaction and sign it with all necessary keys.
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&[signature_secret])?;
        tx.signatures.push(sigs);
        let sigs = tx.create_sigs(&fee_secrets)?;
        tx.signatures.push(sigs);

        Ok(tx)
    }

    /// Import given DAO votes into the wallet
    /// This function is really bad but I'm also really tired and annoyed.
    pub async fn dao_exec(&self, _proposal: &DaoProposalBulla) -> Result<Transaction> {
        // TODO
        unimplemented!()
    }
}
