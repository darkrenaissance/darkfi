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
use rand::rngs::OsRng;
use rusqlite::types::Value;

use darkfi::{
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    util::parse::encode_base10,
    zk::{empty_witnesses, halo2::Field, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Error, Result,
};
use darkfi_dao_contract::{
    client::{make_mint_call, DaoProposeCall, DaoProposeStakeInput, DaoVoteCall, DaoVoteInput},
    model::{DaoAuthCall, DaoBulla, DaoMintParams, DaoProposeParams, DaoVoteParams},
    DaoFunction, DAO_CONTRACT_ZKAS_DAO_MINT_NS, DAO_CONTRACT_ZKAS_DAO_PROPOSE_INPUT_NS,
    DAO_CONTRACT_ZKAS_DAO_PROPOSE_MAIN_NS, DAO_CONTRACT_ZKAS_DAO_VOTE_INPUT_NS,
    DAO_CONTRACT_ZKAS_DAO_VOTE_MAIN_NS,
};
use darkfi_money_contract::{client::OwnCoin, model::TokenId, MoneyFunction};
use darkfi_sdk::{
    bridgetree,
    crypto::{
        poseidon_hash,
        util::{fp_mod_fv, fp_to_u64},
        BaseBlind, Blind, FuncId, FuncRef, Keypair, MerkleNode, MerkleTree, PublicKey, ScalarBlind,
        SecretKey, DAO_CONTRACT_ID, MONEY_CONTRACT_ID,
    },
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{
    async_trait, deserialize, serialize, Encodable, SerialDecodable, SerialEncodable,
};

use crate::{
    convert_named_params,
    error::{WalletDbError, WalletDbResult},
    money::BALANCE_BASE10_DECIMALS,
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
pub const DAO_DAOS_COL_DAO_ID: &str = "dao_id";
pub const DAO_DAOS_COL_NAME: &str = "name";
pub const DAO_DAOS_COL_PROPOSER_LIMIT: &str = "proposer_limit";
pub const DAO_DAOS_COL_QUORUM: &str = "quorum";
pub const DAO_DAOS_COL_APPROVAL_RATIO_BASE: &str = "approval_ratio_base";
pub const DAO_DAOS_COL_APPROVAL_RATIO_QUOT: &str = "approval_ratio_quot";
pub const DAO_DAOS_COL_GOV_TOKEN_ID: &str = "gov_token_id";
pub const DAO_DAOS_COL_SECRET: &str = "secret";
pub const DAO_DAOS_COL_BULLA_BLIND: &str = "bulla_blind";
pub const DAO_DAOS_COL_LEAF_POSITION: &str = "leaf_position";
pub const DAO_DAOS_COL_TX_HASH: &str = "tx_hash";
pub const DAO_DAOS_COL_CALL_INDEX: &str = "call_index";

// DAO_TREES_TABLE
pub const DAO_TREES_COL_DAOS_TREE: &str = "daos_tree";
pub const DAO_TREES_COL_PROPOSALS_TREE: &str = "proposals_tree";

// DAO_COINS_TABLE
pub const _DAO_COINS_COL_COIN_ID: &str = "coin_id";
pub const _DAO_COINS_COL_DAO_ID: &str = "dao_id";

// DAO_PROPOSALS_TABLE
pub const DAO_PROPOSALS_COL_PROPOSAL_ID: &str = "proposal_id";
pub const DAO_PROPOSALS_COL_DAO_ID: &str = "dao_id";
pub const DAO_PROPOSALS_COL_RECV_PUBLIC: &str = "recv_public";
pub const DAO_PROPOSALS_COL_AMOUNT: &str = "amount";
pub const DAO_PROPOSALS_COL_SENDCOIN_TOKEN_ID: &str = "sendcoin_token_id";
pub const DAO_PROPOSALS_COL_BULLA_BLIND: &str = "bulla_blind";
pub const DAO_PROPOSALS_COL_LEAF_POSITION: &str = "leaf_position";
pub const DAO_PROPOSALS_COL_MONEY_SNAPSHOT_TREE: &str = "money_snapshot_tree";
pub const DAO_PROPOSALS_COL_TX_HASH: &str = "tx_hash";
pub const DAO_PROPOSALS_COL_CALL_INDEX: &str = "call_index";
pub const _DAO_PROPOSALS_COL_OUR_VOTE_ID: &str = "our_vote_id";

// DAO_VOTES_TABLE
pub const _DAO_VOTES_COL_VOTE_ID: &str = "vote_id";
pub const DAO_VOTES_COL_PROPOSAL_ID: &str = "proposal_id";
pub const DAO_VOTES_COL_VOTE_OPTION: &str = "vote_option";
pub const DAO_VOTES_COL_YES_VOTE_BLIND: &str = "yes_vote_blind";
pub const DAO_VOTES_COL_ALL_VOTE_VALUE: &str = "all_vote_value";
pub const DAO_VOTES_COL_ALL_VOTE_BLIND: &str = "all_vote_blind";
pub const DAO_VOTES_COL_TX_HASH: &str = "tx_hash";
pub const DAO_VOTES_COL_CALL_INDEX: &str = "call_index";

#[derive(SerialEncodable, SerialDecodable, Clone)]
pub struct DaoProposalInfo {
    pub dest: PublicKey,
    pub amount: u64,
    pub token_id: TokenId,
    pub blind: BaseBlind,
}

#[derive(SerialEncodable, SerialDecodable)]
pub struct DaoProposeNote {
    pub proposal: DaoProposalInfo,
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
/// Parameters representing a DAO to be initialized
pub struct DaoParams {
    /// The minimum amount of governance tokens needed to open a proposal
    pub proposer_limit: u64,
    /// Minimal threshold of participating total tokens needed for a proposal to pass
    pub quorum: u64,
    /// The ratio of winning/total votes needed for a proposal to pass
    pub approval_ratio_base: u64,
    pub approval_ratio_quot: u64,
    /// DAO's governance token ID
    pub gov_token_id: TokenId,
    /// Secret key for the DAO
    pub secret_key: SecretKey,
    /// DAO bulla blind
    pub bulla_blind: pallas::Base,
}

impl fmt::Display for DaoParams {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = format!(
            "{}\n{}\n{}: {} ({})\n{}: {} ({})\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {:?}",
            "DAO Parameters",
            "==============",
            "Proposer limit",
            encode_base10(self.proposer_limit, BALANCE_BASE10_DECIMALS),
            self.proposer_limit,
            "Quorum",
            encode_base10(self.quorum, BALANCE_BASE10_DECIMALS),
            self.quorum,
            "Approval ratio",
            self.approval_ratio_quot as f64 / self.approval_ratio_base as f64,
            "Governance Token ID",
            self.gov_token_id,
            "Public key",
            PublicKey::from_secret(self.secret_key),
            "Secret key",
            self.secret_key,
            "Bulla blind",
            self.bulla_blind,
        );

        write!(f, "{}", s)
    }
}

#[derive(Debug, Clone)]
/// Parameters representing an intialized DAO, optionally deployed on-chain
pub struct Dao {
    /// Numeric identifier for the DAO
    pub id: u64,
    /// Named identifier for the DAO
    pub name: String,
    /// The minimum amount of governance tokens needed to open a proposal
    pub proposer_limit: u64,
    /// Minimal threshold of participating total tokens needed for a proposal to pass
    pub quorum: u64,
    /// The ratio of winning/total votes needed for a proposal to pass
    pub approval_ratio_base: u64,
    pub approval_ratio_quot: u64,
    /// DAO's governance token ID
    pub gov_token_id: TokenId,
    /// Secret key for the DAO
    pub secret_key: SecretKey,
    /// DAO bulla blind
    pub bulla_blind: BaseBlind,
    /// Leaf position of the DAO in the Merkle tree of DAOs
    pub leaf_position: Option<bridgetree::Position>,
    /// The transaction hash where the DAO was deployed
    pub tx_hash: Option<blake3::Hash>,
    /// The call index in the transaction where the DAO was deployed
    pub call_index: Option<u32>,
}

impl Dao {
    pub fn bulla(&self) -> DaoBulla {
        let (x, y) = PublicKey::from_secret(self.secret_key).xy();

        DaoBulla::from(poseidon_hash([
            pallas::Base::from(self.proposer_limit),
            pallas::Base::from(self.quorum),
            pallas::Base::from(self.approval_ratio_quot),
            pallas::Base::from(self.approval_ratio_base),
            self.gov_token_id.inner(),
            x,
            y,
            self.bulla_blind.inner(),
        ]))
    }

    pub fn keypair(&self) -> Keypair {
        let public = PublicKey::from_secret(self.secret_key);
        Keypair { public, secret: self.secret_key }
    }
}

impl fmt::Display for Dao {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = format!(
            "{}\n{}\n{}: {}\n{}: {}\n{}: {} ({})\n{}: {} ({})\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {:?}\n{}: {:?}\n{}: {:?}\n{}: {:?}",
            "DAO Parameters",
            "==============",
            "Name",
            self.name,
            "Bulla",
            self.bulla(),
            "Proposer limit",
            encode_base10(self.proposer_limit, BALANCE_BASE10_DECIMALS),
            self.proposer_limit,
            "Quorum",
            encode_base10(self.quorum, BALANCE_BASE10_DECIMALS),
            self.quorum,
            "Approval ratio",
            self.approval_ratio_quot as f64 / self.approval_ratio_base as f64,
            "Governance Token ID",
            self.gov_token_id,
            "Public key",
            PublicKey::from_secret(self.secret_key),
            "Secret key",
            self.secret_key,
            "Bulla blind",
            self.bulla_blind,
            "Leaf position",
            self.leaf_position,
            "Tx hash",
            self.tx_hash,
            "Call idx",
            self.call_index,
        );

        write!(f, "{}", s)
    }
}

#[derive(Debug, Clone)]
/// Parameters representing an initialized DAO proposal, optionally deployed on-chain
pub struct DaoProposal {
    /// Numeric identifier for the proposal
    pub id: u64,
    /// The DAO bulla related to this proposal
    pub dao_bulla: DaoBulla,
    /// Recipient of this proposal's funds
    pub recipient: PublicKey,
    /// Amount of this proposal
    pub amount: u64,
    /// Token ID to be sent
    pub token_id: TokenId,
    /// Proposal's bulla blind
    pub bulla_blind: BaseBlind,
    /// Leaf position of this proposal in the Merkle tree of proposals
    pub leaf_position: Option<bridgetree::Position>,
    /// Snapshotted Money Merkle tree
    pub money_snapshot_tree: Option<MerkleTree>,
    /// Transaction hash where this proposal was proposed
    pub tx_hash: Option<blake3::Hash>,
    /// call index in the transaction where this proposal was proposed
    pub call_index: Option<u32>,
    /// The vote ID we've voted on this proposal
    pub vote_id: Option<pallas::Base>,
}

impl DaoProposal {
    pub fn bulla(&self) -> pallas::Base {
        let (dest_x, dest_y) = self.recipient.xy();

        poseidon_hash([
            dest_x,
            dest_y,
            pallas::Base::from(self.amount),
            self.token_id.inner(),
            self.dao_bulla.inner(),
            self.bulla_blind.inner(),
        ])
    }
}

impl fmt::Display for DaoProposal {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = format!(
            concat!(
                "Proposal parameters\n",
                "===================\n",
                "DAO Bulla: {}\n",
                "Recipient: {}\n",
                "Proposal amount: {} ({})\n",
                "Proposal Token ID: {:?}\n",
                "Proposal bulla blind: {:?}\n",
                "Proposal leaf position: {:?}\n",
                "Proposal tx hash: {:?}\n",
                "Proposal call index: {:?}\n",
                "Proposal vote ID: {:?}",
            ),
            self.dao_bulla,
            self.recipient,
            encode_base10(self.amount, BALANCE_BASE10_DECIMALS),
            self.amount,
            self.token_id,
            self.bulla_blind,
            self.leaf_position,
            self.tx_hash,
            self.call_index,
            self.vote_id,
        );

        write!(f, "{}", s)
    }
}

#[derive(Debug, Clone)]
/// Parameters representing a vote we've made on a DAO proposal
pub struct DaoVote {
    /// Numeric identifier for the vote
    pub id: u64,
    /// Numeric identifier for the proposal related to this vote
    pub proposal_id: u64,
    /// The vote
    pub vote_option: bool,
    /// Blinding factor for the yes vote
    pub yes_vote_blind: ScalarBlind,
    /// Value of all votes
    pub all_vote_value: u64,
    /// Blinding facfor of all votes
    pub all_vote_blind: ScalarBlind,
    /// Transaction hash where this vote was casted
    pub tx_hash: Option<blake3::Hash>,
    /// call index in the transaction where this vote was casted
    pub call_index: Option<u32>,
}

impl Drk {
    /// Initialize wallet with tables for the DAO contract.
    pub async fn initialize_dao(&self) -> WalletDbResult<()> {
        // Initialize DAO wallet schema
        let wallet_schema = include_str!("../dao.sql");
        self.wallet.exec_batch_sql(wallet_schema).await?;

        // Check if we have to initialize the Merkle trees.
        // We check if one exists, but we actually create two. This should be written
        // a bit better and safer.
        // For now, on success, we don't care what's returned, but in the future
        // we should actually check it.
        if self
            .wallet
            .query_single(&DAO_TREES_TABLE, &[DAO_TREES_COL_DAOS_TREE], &[])
            .await
            .is_err()
        {
            eprintln!("Initializing DAO Merkle trees");
            let tree = MerkleTree::new(100);
            self.put_dao_trees(&tree, &tree).await?;
            eprintln!("Successfully initialized Merkle trees for the DAO contract");
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
        self.wallet.exec_sql(&query, &[]).await?;

        // then we insert the new one
        let query = format!(
            "INSERT INTO {} ({}, {}) VALUES (?1, ?2);",
            *DAO_TREES_TABLE, DAO_TREES_COL_DAOS_TREE, DAO_TREES_COL_PROPOSALS_TREE,
        );
        self.wallet
            .exec_sql(&query, rusqlite::params![serialize(daos_tree), serialize(proposals_tree)])
            .await
    }

    /// Fetch DAO Merkle trees from the wallet.
    pub async fn get_dao_trees(&self) -> Result<(MerkleTree, MerkleTree)> {
        let row = match self.wallet.query_single(&DAO_TREES_TABLE, &[], &[]).await {
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
        let daos_tree = deserialize(daos_tree_bytes)?;

        let Value::Blob(ref proposals_tree_bytes) = row[1] else {
            return Err(Error::ParseFailed("[get_dao_trees] Proposals tree bytes parsing failed"))
        };
        let proposals_tree = deserialize(proposals_tree_bytes)?;

        Ok((daos_tree, proposals_tree))
    }

    /// Fetch all DAO secret keys from the wallet.
    pub async fn get_dao_secrets(&self) -> Result<Vec<SecretKey>> {
        let daos = self.get_daos().await?;
        let mut ret = Vec::with_capacity(daos.len());
        for dao in daos {
            ret.push(dao.secret_key);
        }

        Ok(ret)
    }

    /// Fetch all known DAOs from the wallet.
    pub async fn get_daos(&self) -> Result<Vec<Dao>> {
        let rows = match self.wallet.query_multiple(&DAO_DAOS_TABLE, &[], &[]).await {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::RusqliteError(format!("[get_daos] DAOs retrieval failed: {e:?}")))
            }
        };

        let mut daos = Vec::with_capacity(rows.len());
        for row in rows {
            let Value::Integer(id) = row[0] else {
                return Err(Error::ParseFailed("[get_daos] ID parsing failed"))
            };
            let Ok(id) = u64::try_from(id) else {
                return Err(Error::ParseFailed("[get_daos] ID parsing failed"))
            };

            let Value::Text(ref name) = row[1] else {
                return Err(Error::ParseFailed("[get_daos] Name parsing failed"))
            };
            let name = name.clone();

            let Value::Blob(ref proposer_limit_bytes) = row[2] else {
                return Err(Error::ParseFailed("[get_daos] Proposer limit bytes parsing failed"))
            };
            let proposer_limit = deserialize(proposer_limit_bytes)?;

            let Value::Blob(ref quorum_bytes) = row[3] else {
                return Err(Error::ParseFailed("[get_daos] Quorum bytes parsing failed"))
            };
            let quorum = deserialize(quorum_bytes)?;

            let Value::Integer(approval_ratio_base) = row[4] else {
                return Err(Error::ParseFailed("[get_daos] Approval ratio base parsing failed"))
            };
            let Ok(approval_ratio_base) = u64::try_from(approval_ratio_base) else {
                return Err(Error::ParseFailed("[get_daos] Approval ratio base parsing failed"))
            };

            let Value::Integer(approval_ratio_quot) = row[5] else {
                return Err(Error::ParseFailed("[get_daos] Approval ratio quot parsing failed"))
            };
            let Ok(approval_ratio_quot) = u64::try_from(approval_ratio_quot) else {
                return Err(Error::ParseFailed("[get_daos] Approval ratio quot parsing failed"))
            };

            let Value::Blob(ref gov_token_bytes) = row[6] else {
                return Err(Error::ParseFailed("[get_daos] Gov token bytes parsing failed"))
            };
            let gov_token_id = deserialize(gov_token_bytes)?;

            let Value::Blob(ref secret_bytes) = row[7] else {
                return Err(Error::ParseFailed("[get_daos] Secret key bytes parsing failed"))
            };
            let secret_key = deserialize(secret_bytes)?;

            let Value::Blob(ref bulla_blind_bytes) = row[8] else {
                return Err(Error::ParseFailed("[get_daos] Bulla blind bytes parsing failed"))
            };
            let bulla_blind = deserialize(bulla_blind_bytes)?;

            let Value::Blob(ref leaf_position_bytes) = row[9] else {
                return Err(Error::ParseFailed("[get_daos] Leaf position bytes parsing failed"))
            };
            let leaf_position = if leaf_position_bytes.is_empty() {
                None
            } else {
                Some(deserialize(leaf_position_bytes)?)
            };

            let Value::Blob(ref tx_hash_bytes) = row[10] else {
                return Err(Error::ParseFailed("[get_daos] Transaction hash bytes parsing failed"))
            };
            let tx_hash =
                if tx_hash_bytes.is_empty() { None } else { Some(deserialize(tx_hash_bytes)?) };

            let Value::Integer(call_index) = row[11] else {
                return Err(Error::ParseFailed("[get_daos] Call index parsing failed"))
            };
            let Ok(call_index) = u32::try_from(call_index) else {
                return Err(Error::ParseFailed("[get_daos] Call index parsing failed"))
            };
            let call_index = Some(call_index);

            let dao = Dao {
                id,
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
            };

            daos.push(dao);
        }

        // Here we sort the vec by ID. The SQL SELECT statement does not guarantee
        // this, so just do it here.
        daos.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(daos)
    }

    /// Auxiliary function to parse a proposal record row.
    fn parse_dao_proposal(&self, dao: &Dao, row: &[Value]) -> Result<DaoProposal> {
        let Value::Integer(id) = row[0] else {
            return Err(Error::ParseFailed("[get_dao_proposals] ID parsing failed"))
        };
        let Ok(id) = u64::try_from(id) else {
            return Err(Error::ParseFailed("[get_dao_proposals] ID parsing failed"))
        };

        let Value::Integer(dao_id) = row[1] else {
            return Err(Error::ParseFailed("[get_dao_proposals] DAO ID parsing failed"))
        };
        let Ok(dao_id) = u64::try_from(dao_id) else {
            return Err(Error::ParseFailed("[get_dao_proposals] DAO ID parsing failed"))
        };
        assert!(dao_id == dao.id);
        let dao_bulla = dao.bulla();

        let Value::Blob(ref recipient_bytes) = row[2] else {
            return Err(Error::ParseFailed(
                "[get_dao_proposals] Recipient bytes bytes parsing failed",
            ))
        };
        let recipient = deserialize(recipient_bytes)?;

        let Value::Blob(ref amount_bytes) = row[3] else {
            return Err(Error::ParseFailed("[get_dao_proposals] Amount bytes parsing failed"))
        };
        let amount = deserialize(amount_bytes)?;

        let Value::Blob(ref token_id_bytes) = row[4] else {
            return Err(Error::ParseFailed("[get_dao_proposals] Token ID bytes parsing failed"))
        };
        let token_id = deserialize(token_id_bytes)?;

        let Value::Blob(ref bulla_blind_bytes) = row[5] else {
            return Err(Error::ParseFailed("[get_dao_proposals] Bulla blind bytes parsing failed"))
        };
        let bulla_blind = deserialize(bulla_blind_bytes)?;

        let Value::Blob(ref leaf_position_bytes) = row[6] else {
            return Err(Error::ParseFailed("[get_dao_proposals] Leaf position bytes parsing failed"))
        };
        let leaf_position = if leaf_position_bytes.is_empty() {
            None
        } else {
            Some(deserialize(leaf_position_bytes)?)
        };

        let Value::Blob(ref money_snapshot_tree_bytes) = row[7] else {
            return Err(Error::ParseFailed(
                "[get_dao_proposals] Money snapshot tree bytes parsing failed",
            ))
        };
        let money_snapshot_tree = if money_snapshot_tree_bytes.is_empty() {
            None
        } else {
            Some(deserialize(money_snapshot_tree_bytes)?)
        };

        let Value::Blob(ref tx_hash_bytes) = row[8] else {
            return Err(Error::ParseFailed(
                "[get_dao_proposals] Transaction hash bytes parsing failed",
            ))
        };
        let tx_hash =
            if tx_hash_bytes.is_empty() { None } else { Some(deserialize(tx_hash_bytes)?) };

        let Value::Integer(call_index) = row[9] else {
            return Err(Error::ParseFailed("[get_dao_proposals] Call index parsing failed"))
        };
        let Ok(call_index) = u32::try_from(call_index) else {
            return Err(Error::ParseFailed("[get_dao_proposals] Call index parsing failed"))
        };
        let call_index = Some(call_index);

        let Value::Blob(ref vote_id_bytes) = row[10] else {
            return Err(Error::ParseFailed("[get_dao_proposals] Vote ID bytes parsing failed"))
        };
        let vote_id =
            if vote_id_bytes.is_empty() { None } else { Some(deserialize(vote_id_bytes)?) };

        Ok(DaoProposal {
            id,
            dao_bulla,
            recipient,
            amount,
            token_id,
            bulla_blind,
            leaf_position,
            money_snapshot_tree,
            tx_hash,
            call_index,
            vote_id,
        })
    }

    /// Fetch all known DAO proposals from the wallet given a DAO ID.
    pub async fn get_dao_proposals(&self, dao_id: u64) -> Result<Vec<DaoProposal>> {
        let daos = self.get_daos().await?;
        let Some(dao) = daos.get(dao_id as usize - 1) else {
            return Err(Error::RusqliteError(format!(
                "[get_dao_proposals] DAO with ID {dao_id} not found in wallet"
            )))
        };

        let rows = match self
            .wallet
            .query_multiple(
                &DAO_PROPOSALS_TABLE,
                &[],
                convert_named_params! {(DAO_PROPOSALS_COL_DAO_ID, dao_id)},
            )
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::RusqliteError(format!(
                    "[get_dao_proposals] Proposals retrieval failed: {e:?}"
                )))
            }
        };

        let mut proposals = Vec::with_capacity(rows.len());
        for row in rows {
            let proposal = self.parse_dao_proposal(dao, &row)?;
            proposals.push(proposal);
        }

        // Here we sort the vec by ID. The SQL SELECT statement does not guarantee
        // this, so just do it here.
        proposals.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(proposals)
    }

    /// Append data related to DAO contract transactions into the wallet database.
    /// Optionally, if `confirm` is true, also append the data in the Merkle trees, etc.
    pub async fn apply_tx_dao_data(&self, tx: &Transaction, confirm: bool) -> Result<()> {
        let cid = *DAO_CONTRACT_ID;
        let mut daos = self.get_daos().await?;
        let mut daos_to_confirm = vec![];
        let (mut daos_tree, mut proposals_tree) = self.get_dao_trees().await?;

        // DAOs that have been minted
        let mut new_dao_bullas: Vec<(DaoBulla, Option<blake3::Hash>, u32)> = vec![];
        // DAO proposals that have been minted
        let mut new_dao_proposals: Vec<(
            DaoProposeParams,
            Option<MerkleTree>,
            Option<blake3::Hash>,
            u32,
        )> = vec![];
        let mut our_proposals: Vec<DaoProposal> = vec![];
        // DAO votes that have been seen
        let mut new_dao_votes: Vec<(DaoVoteParams, Option<blake3::Hash>, u32)> = vec![];
        let mut dao_votes: Vec<DaoVote> = vec![];

        // Run through the transaction and see what we got:
        for (i, call) in tx.calls.iter().enumerate() {
            if call.data.contract_id == cid && call.data.data[0] == DaoFunction::Mint as u8 {
                eprintln!("Found Dao::Mint in call {i}");
                let params: DaoMintParams = deserialize(&call.data.data[1..])?;
                let tx_hash = if confirm { Some(blake3::hash(&serialize(tx))) } else { None };
                new_dao_bullas.push((params.dao_bulla, tx_hash, i as u32));
                continue
            }

            if call.data.contract_id == cid && call.data.data[0] == DaoFunction::Propose as u8 {
                eprintln!("Found Dao::Propose in call {i}");
                let params: DaoProposeParams = deserialize(&call.data.data[1..])?;
                let tx_hash = if confirm { Some(blake3::hash(&serialize(tx))) } else { None };
                // We need to clone the tree here for reproducing the snapshot Merkle root
                let money_tree = if confirm { Some(self.get_money_tree().await?) } else { None };
                new_dao_proposals.push((params, money_tree, tx_hash, i as u32));
                continue
            }

            if call.data.contract_id == cid && call.data.data[0] == DaoFunction::Vote as u8 {
                eprintln!("Found Dao::Vote in call {i}");
                let params: DaoVoteParams = deserialize(&call.data.data[1..])?;
                let tx_hash = if confirm { Some(blake3::hash(&serialize(tx))) } else { None };
                new_dao_votes.push((params, tx_hash, i as u32));
                continue
            }

            if call.data.contract_id == cid && call.data.data[0] == DaoFunction::Exec as u8 {
                // This seems to not need any special action
                eprintln!("Found Dao::Exec in call {i}");
                continue
            }
        }

        // This code should only be executed when finalized blocks are being scanned.
        // Here we write the tx metadata, and actually do Merkle tree appends so we
        // have to make sure it's the same for everyone.
        if confirm {
            for new_bulla in new_dao_bullas {
                daos_tree.append(MerkleNode::from(new_bulla.0.inner()));
                for dao in daos.iter_mut() {
                    if dao.bulla() == new_bulla.0 {
                        eprintln!(
                            "Found minted DAO {}, noting down for wallet update",
                            new_bulla.0
                        );
                        // We have this DAO imported in our wallet. Add the metadata:
                        dao.leaf_position = daos_tree.mark();
                        dao.tx_hash = new_bulla.1;
                        dao.call_index = Some(new_bulla.2);
                        daos_to_confirm.push(dao.clone());
                    }
                }
            }

            for proposal in new_dao_proposals {
                proposals_tree.append(MerkleNode::from(proposal.0.proposal_bulla.inner()));

                // If we're able to decrypt this note, that's the way to link it
                // to a specific DAO.
                for dao in &daos {
                    if let Ok(note) = proposal.0.note.decrypt::<DaoProposeNote>(&dao.secret_key) {
                        // We managed to decrypt it. Let's place this in a proper
                        // DaoProposal object. We assume we can just increment the
                        // ID by looking at how many proposals we already have.
                        // We also assume we don't mantain duplicate DAOs in the
                        // wallet.
                        eprintln!("Managed to decrypt DAO proposal note");
                        let daos_proposals = self.get_dao_proposals(dao.id).await?;
                        let our_prop = DaoProposal {
                            // This ID stuff is flaky.
                            id: daos_proposals.len() as u64 + our_proposals.len() as u64 + 1,
                            dao_bulla: dao.bulla(),
                            recipient: note.proposal.dest,
                            amount: note.proposal.amount,
                            token_id: note.proposal.token_id,
                            bulla_blind: note.proposal.blind,
                            leaf_position: proposals_tree.mark(),
                            money_snapshot_tree: proposal.1,
                            tx_hash: proposal.2,
                            call_index: Some(proposal.3),
                            vote_id: None,
                        };

                        our_proposals.push(our_prop);
                        break
                    }
                }
            }

            for vote in new_dao_votes {
                for dao in &daos {
                    // TODO: we shouldn't decrypt with all DAOs here
                    let note = vote.0.note.decrypt_unsafe(&dao.secret_key)?;
                    eprintln!("Managed to decrypt DAO proposal vote note");
                    let daos_proposals = self.get_dao_proposals(dao.id).await?;
                    let mut proposal_id = None;

                    for i in daos_proposals {
                        if i.bulla() == vote.0.proposal_bulla.inner() {
                            proposal_id = Some(i.id);
                            break
                        }
                    }

                    if proposal_id.is_none() {
                        eprintln!("Warning: Decrypted DaoVoteNote but did not find proposal");
                        break
                    }

                    let vote_option = fp_to_u64(note[0]).unwrap();
                    assert!(vote_option == 0 || vote_option == 1);
                    let vote_option = vote_option != 0;
                    let yes_vote_blind = Blind(fp_mod_fv(note[1]));
                    let all_vote_value = fp_to_u64(note[2]).unwrap();
                    let all_vote_blind = Blind(fp_mod_fv(note[3]));

                    let v = DaoVote {
                        id: 0,
                        proposal_id: proposal_id.unwrap(),
                        vote_option,
                        yes_vote_blind,
                        all_vote_value,
                        all_vote_blind,
                        tx_hash: vote.1,
                        call_index: Some(vote.2),
                    };

                    dao_votes.push(v);
                }
            }
        }

        if confirm {
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
            self.put_dao_proposals(&our_proposals).await?;
            if let Err(e) = self.put_dao_votes(&dao_votes).await {
                return Err(Error::RusqliteError(format!(
                    "[apply_tx_dao_data] Put DAO votes failed: {e:?}"
                )))
            }
        }

        Ok(())
    }

    /// Confirm already imported DAO metadata into the wallet.
    /// Here we just write the leaf position, tx hash, and call index.
    /// Panics if the fields are None.
    pub async fn confirm_daos(&self, daos: &[Dao]) -> WalletDbResult<()> {
        for dao in daos {
            let query = format!(
                "UPDATE {} SET {} = ?1, {} = ?2, {} = ?3 WHERE {} = {};",
                *DAO_DAOS_TABLE,
                DAO_DAOS_COL_LEAF_POSITION,
                DAO_DAOS_COL_TX_HASH,
                DAO_DAOS_COL_CALL_INDEX,
                DAO_DAOS_COL_DAO_ID,
                dao.id,
            );
            self.wallet
                .exec_sql(
                    &query,
                    rusqlite::params![
                        serialize(&dao.leaf_position.unwrap()),
                        serialize(&dao.tx_hash.unwrap()),
                        dao.call_index.unwrap()
                    ],
                )
                .await?;
        }

        Ok(())
    }

    /// Unconfirm imported DAOs by removing the leaf position, txid, and call index.
    pub async fn unconfirm_daos(&self, daos: &[Dao]) -> WalletDbResult<()> {
        for dao in daos {
            let query = format!(
                "UPDATE {} SET {} = ?1, {} = ?2, {} = ?3 WHERE {} = {};",
                *DAO_DAOS_TABLE,
                DAO_DAOS_COL_LEAF_POSITION,
                DAO_DAOS_COL_TX_HASH,
                DAO_DAOS_COL_CALL_INDEX,
                DAO_DAOS_COL_DAO_ID,
                dao.id,
            );
            self.wallet
                .exec_sql(&query, rusqlite::params![None::<Vec<u8>>, None::<Vec<u8>>, None::<u64>,])
                .await?;
        }

        Ok(())
    }

    /// Import given DAO proposals into the wallet.
    pub async fn put_dao_proposals(&self, proposals: &[DaoProposal]) -> Result<()> {
        let daos = self.get_daos().await?;

        for proposal in proposals {
            let Some(dao) = daos.iter().find(|x| x.bulla() == proposal.dao_bulla) else {
                return Err(Error::RusqliteError(
                    "[put_dao_proposals] Couldn't find respective DAO".to_string(),
                ))
            };

            let query = format!(
                "INSERT INTO {} ({}, {}, {}, {}, {}, {}, {}, {}, {}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9);",
                *DAO_PROPOSALS_TABLE,
                DAO_PROPOSALS_COL_DAO_ID,
                DAO_PROPOSALS_COL_RECV_PUBLIC,
                DAO_PROPOSALS_COL_AMOUNT,
                DAO_PROPOSALS_COL_SENDCOIN_TOKEN_ID,
                DAO_PROPOSALS_COL_BULLA_BLIND,
                DAO_PROPOSALS_COL_LEAF_POSITION,
                DAO_PROPOSALS_COL_MONEY_SNAPSHOT_TREE,
                DAO_PROPOSALS_COL_TX_HASH,
                DAO_PROPOSALS_COL_CALL_INDEX,
            );

            if let Err(e) = self
                .wallet
                .exec_sql(
                    &query,
                    rusqlite::params![
                        dao.id,
                        serialize(&proposal.recipient),
                        serialize(&proposal.amount),
                        serialize(&proposal.token_id),
                        serialize(&proposal.bulla_blind),
                        serialize(&proposal.leaf_position.unwrap()),
                        serialize(&proposal.money_snapshot_tree.clone().unwrap()),
                        serialize(&proposal.tx_hash.unwrap()),
                        proposal.call_index,
                    ],
                )
                .await
            {
                return Err(Error::RusqliteError(format!(
                    "[put_dao_proposals] Proposal insert failed: {e:?}"
                )))
            };
        }

        Ok(())
    }

    /// Import given DAO votes into the wallet.
    pub async fn put_dao_votes(&self, votes: &[DaoVote]) -> WalletDbResult<()> {
        for vote in votes {
            eprintln!("Importing DAO vote into wallet");

            let query = format!(
                "INSERT INTO {} ({}, {}, {}, {}, {}, {}, {}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7);",
                *DAO_VOTES_TABLE,
                DAO_VOTES_COL_PROPOSAL_ID,
                DAO_VOTES_COL_VOTE_OPTION,
                DAO_VOTES_COL_YES_VOTE_BLIND,
                DAO_VOTES_COL_ALL_VOTE_VALUE,
                DAO_VOTES_COL_ALL_VOTE_BLIND,
                DAO_VOTES_COL_TX_HASH,
                DAO_VOTES_COL_CALL_INDEX,
            );

            self.wallet
                .exec_sql(
                    &query,
                    rusqlite::params![
                        vote.proposal_id,
                        vote.vote_option as u64,
                        serialize(&vote.yes_vote_blind),
                        serialize(&vote.all_vote_value),
                        serialize(&vote.all_vote_blind),
                        serialize(&vote.tx_hash.unwrap()),
                        vote.call_index.unwrap(),
                    ],
                )
                .await?;

            eprintln!("DAO vote added to wallet");
        }

        Ok(())
    }

    /// Reset the DAO Merkle trees in the wallet.
    pub async fn reset_dao_trees(&self) -> WalletDbResult<()> {
        eprintln!("Resetting DAO Merkle trees");
        let tree = MerkleTree::new(100);
        self.put_dao_trees(&tree, &tree).await?;
        eprintln!("Successfully reset DAO Merkle trees");

        Ok(())
    }

    /// Reset confirmed DAOs in the wallet.
    pub async fn reset_daos(&self) -> WalletDbResult<()> {
        eprintln!("Resetting DAO confirmations");
        let daos = match self.get_daos().await {
            Ok(d) => d,
            Err(e) => {
                eprintln!("[reset_daos] DAOs retrieval failed: {e:?}");
                return Err(WalletDbError::GenericError);
            }
        };
        self.unconfirm_daos(&daos).await?;
        eprintln!("Successfully unconfirmed DAOs");

        Ok(())
    }

    /// Reset all DAO proposals in the wallet.
    pub async fn reset_dao_proposals(&self) -> WalletDbResult<()> {
        eprintln!("Resetting DAO proposals");
        let query = format!("DELETE FROM {};", *DAO_PROPOSALS_TABLE);
        self.wallet.exec_sql(&query, &[]).await
    }

    /// Reset all DAO votes in the wallet.
    pub async fn reset_dao_votes(&self) -> WalletDbResult<()> {
        eprintln!("Resetting DAO votes");
        let query = format!("DELETE FROM {};", *DAO_VOTES_TABLE);
        self.wallet.exec_sql(&query, &[]).await
    }

    /// Import given DAO params into the wallet with a given name.
    pub async fn import_dao(&self, dao_name: String, dao_params: DaoParams) -> Result<()> {
        // First let's check if we've imported this DAO with the given name before.
        // TODO: instead of getting all DAOs and filtering in rust,
        // we can use the DB api directly to query for the record
        // and return the error if it exists
        let daos = self.get_daos().await?;
        if daos.iter().any(|x| x.name == dao_name) {
            return Err(Error::RusqliteError(
                "[import_dao] This DAO has already been imported".to_string(),
            ))
        }

        eprintln!("Importing \"{dao_name}\" DAO into the wallet");

        let query = format!(
            "INSERT INTO {} ({}, {}, {}, {}, {}, {}, {}, {}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8);",
            *DAO_DAOS_TABLE,
            DAO_DAOS_COL_NAME,
            DAO_DAOS_COL_PROPOSER_LIMIT,
            DAO_DAOS_COL_QUORUM,
            DAO_DAOS_COL_APPROVAL_RATIO_BASE,
            DAO_DAOS_COL_APPROVAL_RATIO_QUOT,
            DAO_DAOS_COL_GOV_TOKEN_ID,
            DAO_DAOS_COL_SECRET,
            DAO_DAOS_COL_BULLA_BLIND,
        );
        if let Err(e) = self
            .wallet
            .exec_sql(
                &query,
                rusqlite::params![
                    dao_name,
                    serialize(&dao_params.proposer_limit),
                    serialize(&dao_params.quorum),
                    dao_params.approval_ratio_base,
                    dao_params.approval_ratio_quot,
                    serialize(&dao_params.gov_token_id),
                    serialize(&dao_params.secret_key),
                    serialize(&dao_params.bulla_blind),
                ],
            )
            .await
        {
            return Err(Error::RusqliteError(format!("[import_dao] DAO insert failed: {e:?}")))
        };

        Ok(())
    }

    /// Retrieve DAO ID using provided alias filter.
    pub async fn get_dao_id_by_alias(&self, alias_filter: &str) -> Result<u64> {
        let row = match self
            .wallet
            .query_single(
                &DAO_DAOS_TABLE,
                &[DAO_DAOS_COL_DAO_ID],
                convert_named_params! {(DAO_DAOS_COL_NAME, alias_filter)},
            )
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::RusqliteError(format!(
                    "[get_dao_id_by_alias] DAO retrieval failed: {e:?}"
                )))
            }
        };

        let Value::Integer(dao_id) = row[0] else {
            return Err(Error::ParseFailed("[get_dao_id_by_alias] Key ID parsing failed"))
        };
        let Ok(dao_id) = u64::try_from(dao_id) else {
            return Err(Error::ParseFailed("[get_dao_id_by_alias] Key ID parsing failed"))
        };

        Ok(dao_id)
    }

    /// Convenience function. Interprets the alias either as the DAO alias or its ID.
    pub async fn get_dao_id(&self, alias: &str) -> Result<u64> {
        if let Ok(id) = self.get_dao_id_by_alias(alias).await {
            return Ok(id)
        }
        Ok(alias.parse()?)
    }

    /// Fetch a DAO given a numeric ID.
    pub async fn get_dao_by_id(&self, dao_id: u64) -> Result<Dao> {
        // TODO: instead of getting all DAOs and filtering in rust,
        // we can use the DB api directly to query for the record
        // and then parse it
        let daos = self.get_daos().await?;

        let Some(dao) = daos.iter().find(|x| x.id == dao_id) else {
            return Err(Error::RusqliteError("[get_dao_by_id] DAO not found in wallet".to_string()))
        };

        Ok(dao.clone())
    }

    /// List DAO(s) imported in the wallet. If an ID is given, just print the
    /// metadata for that specific one, if found.
    pub async fn dao_list(&self, dao_id: Option<u64>) -> Result<()> {
        if let Some(dao_id) = dao_id {
            return self.dao_list_single(dao_id).await
        }

        let daos = self.get_daos().await?;
        for dao in daos {
            eprintln!("[{}] {}", dao.id, dao.name);
        }

        Ok(())
    }

    /// Retrieve DAO for provided ID and print its metadata.
    async fn dao_list_single(&self, dao_id: u64) -> Result<()> {
        let dao = self.get_dao_by_id(dao_id).await?;

        eprintln!("{dao}");

        Ok(())
    }

    /// Fetch known unspent balances from the wallet for the given DAO ID
    pub async fn dao_balance(&self, dao_id: u64) -> Result<HashMap<String, u64>> {
        // TODO: instead of getting all DAOs and filtering in rust,
        // we can use the DB api directly to query for the record
        // and then parse it
        let daos = self.get_daos().await?;
        let Some(dao) = daos.get(dao_id as usize - 1) else {
            return Err(Error::RusqliteError(format!("DAO with ID {dao_id} not found in wallet")))
        };

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

    /// Fetch a DAO proposal by its ID
    pub async fn get_dao_proposal_by_id(&self, proposal_id: u64) -> Result<DaoProposal> {
        // Grab the proposal record
        let row = match self
            .wallet
            .query_single(
                &DAO_PROPOSALS_TABLE,
                &[],
                convert_named_params! {(DAO_PROPOSALS_COL_PROPOSAL_ID, proposal_id)},
            )
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::RusqliteError(format!(
                    "[get_dao_proposal_by_id] DAO proposal retrieval failed: {e:?}"
                )))
            }
        };

        // Parse DAO ID to grab the DAO record
        let Value::Integer(dao_id) = row[1] else {
            return Err(Error::ParseFailed("[get_dao_proposal_by_id] DAO ID parsing failed"))
        };
        let Ok(dao_id) = u64::try_from(dao_id) else {
            return Err(Error::ParseFailed("[get_dao_proposal_by_id] DAO ID parsing failed"))
        };
        let dao = self.get_dao_by_id(dao_id).await?;

        // Parse rest of the record
        self.parse_dao_proposal(&dao, &row)
    }

    // Fetch all known DAO proposal votes from the wallet given a proposal ID
    pub async fn get_dao_proposal_votes(&self, proposal_id: u64) -> Result<Vec<DaoVote>> {
        let rows = match self
            .wallet
            .query_multiple(
                &DAO_VOTES_TABLE,
                &[],
                convert_named_params! {(DAO_VOTES_COL_PROPOSAL_ID, proposal_id)},
            )
            .await
        {
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

            let Value::Integer(proposal_id) = row[1] else {
                return Err(Error::ParseFailed(
                    "[get_dao_proposal_votes] Proposal ID parsing failed",
                ))
            };
            let Ok(proposal_id) = u64::try_from(proposal_id) else {
                return Err(Error::ParseFailed(
                    "[get_dao_proposal_votes] Proposal ID parsing failed",
                ))
            };

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
            let yes_vote_blind = deserialize(yes_vote_blind_bytes)?;

            let Value::Blob(ref all_vote_value_bytes) = row[4] else {
                return Err(Error::ParseFailed(
                    "[get_dao_proposal_votes] All vote value bytes parsing failed",
                ))
            };
            let all_vote_value = deserialize(all_vote_value_bytes)?;

            let Value::Blob(ref all_vote_blind_bytes) = row[5] else {
                return Err(Error::ParseFailed(
                    "[get_dao_proposal_votes] All vote blind bytes parsing failed",
                ))
            };
            let all_vote_blind = deserialize(all_vote_blind_bytes)?;

            let Value::Blob(ref tx_hash_bytes) = row[6] else {
                return Err(Error::ParseFailed(
                    "[get_dao_proposal_votes] Transaction hash bytes parsing failed",
                ))
            };
            let tx_hash =
                if tx_hash_bytes.is_empty() { None } else { Some(deserialize(tx_hash_bytes)?) };

            let Value::Integer(call_index) = row[7] else {
                return Err(Error::ParseFailed("[get_dao_proposal_votes] Call index parsing failed"))
            };
            let Ok(call_index) = u32::try_from(call_index) else {
                return Err(Error::ParseFailed("[get_dao_proposal_votes] Call index parsing failed"))
            };
            let call_index = Some(call_index);

            let vote = DaoVote {
                id,
                proposal_id,
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

    /// Mint a DAO on-chain
    pub async fn dao_mint(&self, dao_id: u64) -> Result<Transaction> {
        let dao = self.get_dao_by_id(dao_id).await?;

        if dao.tx_hash.is_some() {
            return Err(Error::Custom(
                "[dao_mint] This DAO seems to have already been minted on-chain".to_string(),
            ))
        }

        // TODO: Simplify this model struct import once
        // we use the structs from contract everwhere
        let dao_info = darkfi_dao_contract::model::Dao {
            proposer_limit: dao.proposer_limit,
            quorum: dao.quorum,
            approval_ratio_base: dao.approval_ratio_base,
            approval_ratio_quot: dao.approval_ratio_quot,
            gov_token_id: dao.gov_token_id,
            public_key: PublicKey::from_secret(dao.secret_key),
            bulla_blind: dao.bulla_blind,
        };

        let zkas_bins = self.lookup_zkas(&DAO_CONTRACT_ID).await?;
        let Some(dao_mint_zkbin) = zkas_bins.iter().find(|x| x.0 == DAO_CONTRACT_ZKAS_DAO_MINT_NS)
        else {
            return Err(Error::RusqliteError("[dao_mint] DAO Mint circuit not found".to_string()))
        };

        let dao_mint_zkbin = ZkBinary::decode(&dao_mint_zkbin.1)?;
        let dao_mint_circuit = ZkCircuit::new(empty_witnesses(&dao_mint_zkbin)?, &dao_mint_zkbin);
        eprintln!("Creating DAO Mint proving key");
        let dao_mint_pk = ProvingKey::build(dao_mint_zkbin.k, &dao_mint_circuit);

        let (params, proofs) =
            make_mint_call(&dao_info, &dao.secret_key, &dao_mint_zkbin, &dao_mint_pk)?;

        let mut data = vec![DaoFunction::Mint as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: *DAO_CONTRACT_ID, data };
        let mut tx_builder = TransactionBuilder::new(ContractCallLeaf { call, proofs }, vec![])?;
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&[dao.secret_key])?;
        tx.signatures = vec![sigs];

        Ok(tx)
    }

    /// Create a DAO proposal
    pub async fn dao_propose(
        &self,
        dao_id: u64,
        _recipient: PublicKey,
        amount: u64,
        token_id: TokenId,
    ) -> Result<Transaction> {
        let Ok(dao) = self.get_dao_by_id(dao_id).await else {
            return Err(Error::RusqliteError("[dao_propose] DAO not found in wallet".to_string()))
        };

        if dao.leaf_position.is_none() || dao.tx_hash.is_none() {
            return Err(Error::Custom(
                "[dao_propose] DAO seems to not have been deployed yet".to_string(),
            ))
        }

        let bulla = dao.bulla();
        let owncoins = self.get_coins(false).await?;

        let dao_spend_hook =
            FuncRef { contract_id: *DAO_CONTRACT_ID, func_code: DaoFunction::Exec as u8 }
                .to_func_id();

        let mut dao_owncoins: Vec<OwnCoin> = owncoins.iter().map(|x| x.0.clone()).collect();
        dao_owncoins.retain(|x| {
            x.note.token_id == token_id &&
                x.note.spend_hook == dao_spend_hook &&
                x.note.user_data == bulla.inner()
        });

        let mut gov_owncoins: Vec<OwnCoin> = owncoins.iter().map(|x| x.0.clone()).collect();
        gov_owncoins.retain(|x| x.note.token_id == dao.gov_token_id);

        if dao_owncoins.is_empty() {
            return Err(Error::Custom(format!(
                "[dao_propose] Did not find any {token_id} coins owned by this DAO"
            )))
        }

        if gov_owncoins.is_empty() {
            return Err(Error::Custom(format!(
                "[dao_propose] Did not find any governance {} coins in wallet",
                dao.gov_token_id
            )))
        }

        if dao_owncoins.iter().map(|x| x.note.value).sum::<u64>() < amount {
            return Err(Error::Custom(format!(
                "[dao_propose] Not enough DAO balance for token ID: {}",
                token_id
            )))
        }

        if gov_owncoins.iter().map(|x| x.note.value).sum::<u64>() < dao.proposer_limit {
            return Err(Error::Custom(format!(
                "[dao_propose] Not enough gov token {} balance to propose",
                dao.gov_token_id
            )))
        }

        // FIXME: Here we're looking for a coin == proposer_limit but this shouldn't have to
        // be the case {
        let Some(gov_coin) = gov_owncoins.iter().find(|x| x.note.value == dao.proposer_limit)
        else {
            return Err(Error::Custom(format!(
                "[dao_propose] Did not find a single gov coin of value {}",
                dao.proposer_limit
            )))
        };
        // }

        // Lookup the zkas bins
        let zkas_bins = self.lookup_zkas(&DAO_CONTRACT_ID).await?;
        let Some(propose_burn_zkbin) =
            zkas_bins.iter().find(|x| x.0 == DAO_CONTRACT_ZKAS_DAO_PROPOSE_INPUT_NS)
        else {
            return Err(Error::Custom("[dao_propose] Propose Burn circuit not found".to_string()))
        };

        let Some(propose_main_zkbin) =
            zkas_bins.iter().find(|x| x.0 == DAO_CONTRACT_ZKAS_DAO_PROPOSE_MAIN_NS)
        else {
            return Err(Error::Custom("[dao_propose] Propose Main circuit not found".to_string()))
        };

        let propose_burn_zkbin = ZkBinary::decode(&propose_burn_zkbin.1)?;
        let propose_main_zkbin = ZkBinary::decode(&propose_main_zkbin.1)?;

        let propose_burn_circuit =
            ZkCircuit::new(empty_witnesses(&propose_burn_zkbin)?, &propose_burn_zkbin);
        let propose_main_circuit =
            ZkCircuit::new(empty_witnesses(&propose_main_zkbin)?, &propose_main_zkbin);

        eprintln!("Creating Propose Burn circuit proving key");
        let propose_burn_pk = ProvingKey::build(propose_burn_zkbin.k, &propose_burn_circuit);
        eprintln!("Creating Propose Main circuit proving key");
        let propose_main_pk = ProvingKey::build(propose_main_zkbin.k, &propose_main_circuit);

        // Now create the parameters for the proposal tx
        let signature_secret = SecretKey::random(&mut OsRng);

        // Get the Merkle path for the gov coin in the money tree
        let money_merkle_tree = self.get_money_tree().await?;
        let gov_coin_merkle_path = money_merkle_tree.witness(gov_coin.leaf_position, 0).unwrap();

        // Fetch the daos Merkle tree
        let (daos_tree, _) = self.get_dao_trees().await?;

        let input = DaoProposeStakeInput {
            secret: gov_coin.secret, // <-- TODO: Is this correct?
            note: gov_coin.note.clone(),
            leaf_position: gov_coin.leaf_position,
            merkle_path: gov_coin_merkle_path,
            signature_secret,
        };

        let (dao_merkle_path, dao_merkle_root) = {
            let root = daos_tree.root(0).unwrap();
            let leaf_pos = dao.leaf_position.unwrap();
            let dao_merkle_path = daos_tree.witness(leaf_pos, 0).unwrap();
            (dao_merkle_path, root)
        };

        // TODO:
        /*
        // Convert coin_params to actual coins
        let mut proposal_coins = vec![];
        for coin_params in proposal_coinattrs {
            proposal_coins.push(coin_params.to_coin());
        }
        */
        let proposal_data = vec![];
        //proposal_coins.encode(&mut proposal_data).unwrap();

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

        // TODO: get current height to calculate day
        // Also contract must check we don't mint a proposal that its creation day is
        // less than current height

        // TODO: Simplify this model struct import once
        // we use the structs from contract everwhere
        let proposal = darkfi_dao_contract::model::DaoProposal {
            auth_calls,
            creation_day: 0,
            duration_days: 30,
            user_data: pallas::Base::ZERO,
            dao_bulla: dao.bulla(),
            blind: Blind::random(&mut OsRng),
        };

        // TODO: Simplify this model struct import once
        // we use the structs from contract everwhere
        let daoinfo = darkfi_dao_contract::model::Dao {
            proposer_limit: dao.proposer_limit,
            quorum: dao.quorum,
            approval_ratio_quot: dao.approval_ratio_quot,
            approval_ratio_base: dao.approval_ratio_base,
            gov_token_id: dao.gov_token_id,
            public_key: PublicKey::from_secret(dao.secret_key),
            bulla_blind: dao.bulla_blind,
        };

        let call = DaoProposeCall {
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
        let call = ContractCall { contract_id: *DAO_CONTRACT_ID, data };
        let mut tx_builder = TransactionBuilder::new(ContractCallLeaf { call, proofs }, vec![])?;
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&[signature_secret])?;
        tx.signatures = vec![sigs];

        Ok(tx)
    }

    /// Vote on a DAO proposal
    pub async fn dao_vote(
        &self,
        dao_id: u64,
        proposal_id: u64,
        vote_option: bool,
        weight: u64,
    ) -> Result<Transaction> {
        let dao = self.get_dao_by_id(dao_id).await?;
        let proposals = self.get_dao_proposals(dao_id).await?;
        let Some(proposal) = proposals.iter().find(|x| x.id == proposal_id) else {
            return Err(Error::Custom("[dao_vote] Proposal ID not found".to_string()))
        };

        let money_tree = proposal.money_snapshot_tree.clone().unwrap();

        let mut coins: Vec<OwnCoin> =
            self.get_coins(false).await?.iter().map(|x| x.0.clone()).collect();

        coins.retain(|x| x.note.token_id == dao.gov_token_id);
        coins.retain(|x| x.note.spend_hook == FuncId::none());

        if coins.iter().map(|x| x.note.value).sum::<u64>() < weight {
            return Err(Error::Custom("[dao_vote] Not enough balance for vote weight".to_string()))
        }

        // TODO: The spent coins need to either be marked as spent here, and/or on scan
        let mut spent_value = 0;
        let mut spent_coins = vec![];
        let mut inputs = vec![];
        let mut input_secrets = vec![];

        // FIXME: We don't take back any change so it's possible to vote with > requested weight.
        for coin in coins {
            if spent_value >= weight {
                break
            }

            spent_value += coin.note.value;
            spent_coins.push(coin.clone());

            let signature_secret = SecretKey::random(&mut OsRng);
            input_secrets.push(signature_secret);

            let leaf_position = coin.leaf_position;
            let merkle_path = money_tree.witness(coin.leaf_position, 0).unwrap();

            let input = DaoVoteInput {
                secret: coin.secret,
                note: coin.note.clone(),
                leaf_position,
                merkle_path,
                signature_secret,
            };

            inputs.push(input);
        }

        // We use the DAO secret to encrypt the vote.
        let dao_keypair = Keypair::new(dao.secret_key);

        // TODO: Fix this
        // TODO: Simplify this model struct import once
        // we use the structs from contract everwhere
        let proposal = darkfi_dao_contract::model::DaoProposal {
            auth_calls: vec![],
            creation_day: 0,
            duration_days: 30,
            user_data: pallas::Base::ZERO,
            dao_bulla: dao.bulla(),
            blind: Blind::random(&mut OsRng),
        };

        // TODO: Simplify this model struct import once
        // we use the structs from contract everwhere
        let dao_info = darkfi_dao_contract::model::Dao {
            proposer_limit: dao.proposer_limit,
            quorum: dao.quorum,
            approval_ratio_quot: dao.approval_ratio_quot,
            approval_ratio_base: dao.approval_ratio_base,
            gov_token_id: dao.gov_token_id,
            public_key: PublicKey::from_secret(dao.secret_key),
            bulla_blind: dao.bulla_blind,
        };

        // TODO: get current height to calculate day

        let call = DaoVoteCall {
            inputs,
            vote_option,
            current_day: 0,
            dao_keypair,
            proposal,
            dao: dao_info,
        };

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

        eprintln!("Creating DAO Vote Burn proving key");
        let dao_vote_burn_pk = ProvingKey::build(dao_vote_burn_zkbin.k, &dao_vote_burn_circuit);
        eprintln!("Creating DAO Vote Main proving key");
        let dao_vote_main_pk = ProvingKey::build(dao_vote_main_zkbin.k, &dao_vote_main_circuit);

        let (params, proofs) = call.make(
            &dao_vote_burn_zkbin,
            &dao_vote_burn_pk,
            &dao_vote_main_zkbin,
            &dao_vote_main_pk,
        )?;

        let mut data = vec![DaoFunction::Vote as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: *DAO_CONTRACT_ID, data };
        let mut tx_builder = TransactionBuilder::new(ContractCallLeaf { call, proofs }, vec![])?;
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&input_secrets)?;
        tx.signatures = vec![sigs];

        Ok(tx)
    }

    /// Import given DAO votes into the wallet
    /// This function is really bad but I'm also really tired and annoyed.
    pub async fn dao_exec(&self, _dao: Dao, _proposal: DaoProposal) -> Result<Transaction> {
        // TODO
        unimplemented!()
    }
}
