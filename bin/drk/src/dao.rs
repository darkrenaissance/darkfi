/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

use std::{collections::HashMap, fmt, str::FromStr};

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
    client::{
        make_mint_call, DaoAuthMoneyTransferCall, DaoExecCall, DaoProposeCall,
        DaoProposeStakeInput, DaoVoteCall, DaoVoteInput,
    },
    model::{
        Dao, DaoAuthCall, DaoBulla, DaoExecParams, DaoMintParams, DaoProposal, DaoProposalBulla,
        DaoProposeParams, DaoVoteParams,
    },
    DaoFunction, DAO_CONTRACT_ZKAS_DAO_AUTH_MONEY_TRANSFER_ENC_COIN_NS,
    DAO_CONTRACT_ZKAS_DAO_AUTH_MONEY_TRANSFER_NS, DAO_CONTRACT_ZKAS_DAO_EARLY_EXEC_NS,
    DAO_CONTRACT_ZKAS_DAO_EXEC_NS, DAO_CONTRACT_ZKAS_DAO_MINT_NS,
    DAO_CONTRACT_ZKAS_DAO_PROPOSE_INPUT_NS, DAO_CONTRACT_ZKAS_DAO_PROPOSE_MAIN_NS,
    DAO_CONTRACT_ZKAS_DAO_VOTE_INPUT_NS, DAO_CONTRACT_ZKAS_DAO_VOTE_MAIN_NS,
};
use darkfi_money_contract::{
    client::transfer_v1::{select_coins, TransferCallBuilder, TransferCallInput},
    model::{CoinAttributes, Nullifier, TokenId},
    MoneyFunction, MONEY_CONTRACT_ZKAS_BURN_NS_V1, MONEY_CONTRACT_ZKAS_FEE_NS_V1,
    MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};
use darkfi_sdk::{
    bridgetree,
    crypto::{
        poseidon_hash,
        smt::{MemoryStorageFp, PoseidonFp, SmtMemoryFp, EMPTY_NODES_FP},
        util::{fp_mod_fv, fp_to_u64},
        BaseBlind, Blind, FuncId, FuncRef, MerkleNode, MerkleTree, PublicKey, ScalarBlind,
        SecretKey, DAO_CONTRACT_ID, MONEY_CONTRACT_ID,
    },
    dark_tree::DarkTree,
    pasta::pallas,
    tx::TransactionHash,
    ContractCall,
};
use darkfi_serial::{
    async_trait, deserialize_async, serialize_async, AsyncEncodable, SerialDecodable,
    SerialEncodable,
};

use crate::{
    cache::{CacheOverlay, CacheSmt, CacheSmtStorage, SLED_MONEY_SMT_TREE},
    convert_named_params,
    error::{WalletDbError, WalletDbResult},
    money::BALANCE_BASE10_DECIMALS,
    rpc::ScanCache,
    Drk,
};

// DAO Merkle trees Sled keys
pub const SLED_MERKLE_TREES_DAO_DAOS: &[u8] = b"_dao_daos";
pub const SLED_MERKLE_TREES_DAO_PROPOSALS: &[u8] = b"_dao_proposals";

// Wallet SQL table constant names. These have to represent the `dao.sql`
// SQL schema. Table names are prefixed with the contract ID to avoid collisions.
lazy_static! {
    pub static ref DAO_DAOS_TABLE: String = format!("{}_dao_daos", DAO_CONTRACT_ID.to_string());
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

// DAO_PROPOSALS_TABLE
pub const DAO_PROPOSALS_COL_BULLA: &str = "bulla";
pub const DAO_PROPOSALS_COL_DAO_BULLA: &str = "dao_bulla";
pub const DAO_PROPOSALS_COL_PROPOSAL: &str = "proposal";
pub const DAO_PROPOSALS_COL_DATA: &str = "data";
pub const DAO_PROPOSALS_COL_LEAF_POSITION: &str = "leaf_position";
pub const DAO_PROPOSALS_COL_MONEY_SNAPSHOT_TREE: &str = "money_snapshot_tree";
pub const DAO_PROPOSALS_COL_NULLIFIERS_SMT_SNAPSHOT: &str = "nullifiers_smt_snapshot";
pub const DAO_PROPOSALS_COL_MINT_HEIGHT: &str = "mint_height";
pub const DAO_PROPOSALS_COL_TX_HASH: &str = "tx_hash";
pub const DAO_PROPOSALS_COL_CALL_INDEX: &str = "call_index";
pub const DAO_PROPOSALS_COL_EXEC_HEIGHT: &str = "exec_height";
pub const DAO_PROPOSALS_COL_EXEC_TX_HASH: &str = "exec_tx_hash";

// DAO_VOTES_TABLE
pub const DAO_VOTES_COL_PROPOSAL_BULLA: &str = "proposal_bulla";
pub const DAO_VOTES_COL_VOTE_OPTION: &str = "vote_option";
pub const DAO_VOTES_COL_YES_VOTE_BLIND: &str = "yes_vote_blind";
pub const DAO_VOTES_COL_ALL_VOTE_VALUE: &str = "all_vote_value";
pub const DAO_VOTES_COL_ALL_VOTE_BLIND: &str = "all_vote_blind";
pub const DAO_VOTES_COL_BLOCK_HEIGHT: &str = "block_height";
pub const DAO_VOTES_COL_TX_HASH: &str = "tx_hash";
pub const DAO_VOTES_COL_CALL_INDEX: &str = "call_index";
pub const DAO_VOTES_COL_NULLIFIERS: &str = "nullifiers";

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
/// Parameters representing a DAO to be initialized
pub struct DaoParams {
    /// The on chain representation of the DAO
    pub dao: Dao,
    /// DAO notes decryption secret key
    pub notes_secret_key: Option<SecretKey>,
    /// DAO proposals creator secret key
    pub proposer_secret_key: Option<SecretKey>,
    /// DAO proposals viewer secret key
    pub proposals_secret_key: Option<SecretKey>,
    /// DAO votes viewer secret key
    pub votes_secret_key: Option<SecretKey>,
    /// DAO proposals executor secret key
    pub exec_secret_key: Option<SecretKey>,
    /// DAO strongly supported proposals executor secret key
    pub early_exec_secret_key: Option<SecretKey>,
}

impl DaoParams {
    /// Generate new `DaoParams`. If a specific secret key is provided,
    /// the corresponding public key will be derived from it and ignore the provided one.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        proposer_limit: u64,
        quorum: u64,
        early_exec_quorum: u64,
        approval_ratio_base: u64,
        approval_ratio_quot: u64,
        gov_token_id: TokenId,
        notes_secret_key: Option<SecretKey>,
        notes_public_key: PublicKey,
        proposer_secret_key: Option<SecretKey>,
        proposer_public_key: PublicKey,
        proposals_secret_key: Option<SecretKey>,
        proposals_public_key: PublicKey,
        votes_secret_key: Option<SecretKey>,
        votes_public_key: PublicKey,
        exec_secret_key: Option<SecretKey>,
        exec_public_key: PublicKey,
        early_exec_secret_key: Option<SecretKey>,
        early_exec_public_key: PublicKey,
        bulla_blind: BaseBlind,
    ) -> Self {
        // Derive corresponding keys from their secret or use the provided ones.
        let notes_public_key = match notes_secret_key {
            Some(secret_key) => PublicKey::from_secret(secret_key),
            None => notes_public_key,
        };
        let proposer_public_key = match proposer_secret_key {
            Some(secret_key) => PublicKey::from_secret(secret_key),
            None => proposer_public_key,
        };
        let proposals_public_key = match proposals_secret_key {
            Some(secret_key) => PublicKey::from_secret(secret_key),
            None => proposals_public_key,
        };
        let votes_public_key = match votes_secret_key {
            Some(secret_key) => PublicKey::from_secret(secret_key),
            None => votes_public_key,
        };
        let exec_public_key = match exec_secret_key {
            Some(secret_key) => PublicKey::from_secret(secret_key),
            None => exec_public_key,
        };
        let early_exec_public_key = match early_exec_secret_key {
            Some(secret_key) => PublicKey::from_secret(secret_key),
            None => early_exec_public_key,
        };

        let dao = Dao {
            proposer_limit,
            quorum,
            early_exec_quorum,
            approval_ratio_base,
            approval_ratio_quot,
            gov_token_id,
            notes_public_key,
            proposer_public_key,
            proposals_public_key,
            votes_public_key,
            exec_public_key,
            early_exec_public_key,
            bulla_blind,
        };
        Self {
            dao,
            notes_secret_key,
            proposer_secret_key,
            proposals_secret_key,
            votes_secret_key,
            exec_secret_key,
            early_exec_secret_key,
        }
    }

    /// Parse provided toml string into `DaoParams`.
    /// If a specific secret key is provided, the corresponding public key
    /// will be derived from it and ignore the provided one.
    pub fn from_toml_str(toml: &str) -> Result<Self> {
        // Parse TOML file contents
        let Ok(contents) = toml::from_str::<toml::Value>(toml) else {
            return Err(Error::ParseFailed("Failed parsing TOML config"))
        };
        let Some(table) = contents.as_table() else {
            return Err(Error::ParseFailed("TOML not a map"))
        };

        // Grab configuration parameters
        let Some(proposer_limit) = table.get("proposer_limit") else {
            return Err(Error::ParseFailed("TOML does not contain proposer limit"))
        };
        let Some(proposer_limit) = proposer_limit.as_str() else {
            return Err(Error::ParseFailed("Invalid proposer limit: Not a string"))
        };
        if f64::from_str(proposer_limit).is_err() {
            return Err(Error::ParseFailed("Invalid proposer limit: Cannot be parsed to float"))
        }
        let proposer_limit = decode_base10(proposer_limit, BALANCE_BASE10_DECIMALS, true)?;

        let Some(quorum) = table.get("quorum") else {
            return Err(Error::ParseFailed("TOML does not contain quorum"))
        };
        let Some(quorum) = quorum.as_str() else {
            return Err(Error::ParseFailed("Invalid quorum: Not a string"))
        };
        if f64::from_str(quorum).is_err() {
            return Err(Error::ParseFailed("Invalid quorum: Cannot be parsed to float"))
        }
        let quorum = decode_base10(quorum, BALANCE_BASE10_DECIMALS, true)?;

        let Some(early_exec_quorum) = table.get("early_exec_quorum") else {
            return Err(Error::ParseFailed("TOML does not contain early exec quorum"))
        };
        let Some(early_exec_quorum) = early_exec_quorum.as_str() else {
            return Err(Error::ParseFailed("Invalid early exec quorum: Not a string"))
        };
        if f64::from_str(early_exec_quorum).is_err() {
            return Err(Error::ParseFailed("Invalid early exec quorum: Cannot be parsed to float"))
        }
        let early_exec_quorum = decode_base10(early_exec_quorum, BALANCE_BASE10_DECIMALS, true)?;

        let Some(approval_ratio) = table.get("approval_ratio") else {
            return Err(Error::ParseFailed("TOML does not contain approval ratio"))
        };
        let Some(approval_ratio) = approval_ratio.as_float() else {
            return Err(Error::ParseFailed("Invalid approval ratio: Not a float"))
        };
        if approval_ratio > 1.0 {
            return Err(Error::ParseFailed("Approval ratio cannot be >1.0"))
        }
        let approval_ratio_base = 100_u64;
        let approval_ratio_quot = (approval_ratio * approval_ratio_base as f64) as u64;

        let Some(gov_token_id) = table.get("gov_token_id") else {
            return Err(Error::ParseFailed("TOML does not contain gov token id"))
        };
        let Some(gov_token_id) = gov_token_id.as_str() else {
            return Err(Error::ParseFailed("Invalid gov token id: Not a string"))
        };
        let gov_token_id = TokenId::from_str(gov_token_id)?;

        let Some(bulla_blind) = table.get("bulla_blind") else {
            return Err(Error::ParseFailed("TOML does not contain bulla blind"))
        };
        let Some(bulla_blind) = bulla_blind.as_str() else {
            return Err(Error::ParseFailed("Invalid bulla blind: Not a string"))
        };
        let bulla_blind = BaseBlind::from_str(bulla_blind)?;

        // Grab DAO actions keypairs
        let notes_secret_key = match table.get("notes_secret_key") {
            Some(notes_secret_key) => {
                let Some(notes_secret_key) = notes_secret_key.as_str() else {
                    return Err(Error::ParseFailed("Invalid notes secret key: Not a string"))
                };
                let Ok(notes_secret_key) = SecretKey::from_str(notes_secret_key) else {
                    return Err(Error::ParseFailed("Invalid notes secret key: Decoding failed"))
                };
                Some(notes_secret_key)
            }
            None => None,
        };
        let notes_public_key = match notes_secret_key {
            Some(notes_secret_key) => PublicKey::from_secret(notes_secret_key),
            None => {
                let Some(notes_public_key) = table.get("notes_public_key") else {
                    return Err(Error::ParseFailed("TOML does not contain notes public key"))
                };
                let Some(notes_public_key) = notes_public_key.as_str() else {
                    return Err(Error::ParseFailed("Invalid notes public key: Not a string"))
                };
                let Ok(notes_public_key) = PublicKey::from_str(notes_public_key) else {
                    return Err(Error::ParseFailed("Invalid notes public key: Decoding failed"))
                };
                notes_public_key
            }
        };

        let proposer_secret_key = match table.get("proposer_secret_key") {
            Some(proposer_secret_key) => {
                let Some(proposer_secret_key) = proposer_secret_key.as_str() else {
                    return Err(Error::ParseFailed("Invalid proposer secret key: Not a string"))
                };
                let Ok(proposer_secret_key) = SecretKey::from_str(proposer_secret_key) else {
                    return Err(Error::ParseFailed("Invalid proposer secret key: Decoding failed"))
                };
                Some(proposer_secret_key)
            }
            None => None,
        };
        let proposer_public_key = match proposer_secret_key {
            Some(proposer_secret_key) => PublicKey::from_secret(proposer_secret_key),
            None => {
                let Some(proposer_public_key) = table.get("proposer_public_key") else {
                    return Err(Error::ParseFailed("TOML does not contain proposer public key"))
                };
                let Some(proposer_public_key) = proposer_public_key.as_str() else {
                    return Err(Error::ParseFailed("Invalid proposer public key: Not a string"))
                };
                let Ok(proposer_public_key) = PublicKey::from_str(proposer_public_key) else {
                    return Err(Error::ParseFailed("Invalid proposer public key: Decoding failed"))
                };
                proposer_public_key
            }
        };

        let proposals_secret_key = match table.get("proposals_secret_key") {
            Some(proposals_secret_key) => {
                let Some(proposals_secret_key) = proposals_secret_key.as_str() else {
                    return Err(Error::ParseFailed("Invalid proposals secret key: Not a string"))
                };
                let Ok(proposals_secret_key) = SecretKey::from_str(proposals_secret_key) else {
                    return Err(Error::ParseFailed("Invalid proposals secret key: Decoding failed"))
                };
                Some(proposals_secret_key)
            }
            None => None,
        };
        let proposals_public_key = match proposals_secret_key {
            Some(proposals_secret_key) => PublicKey::from_secret(proposals_secret_key),
            None => {
                let Some(proposals_public_key) = table.get("proposals_public_key") else {
                    return Err(Error::ParseFailed("TOML does not contain proposals public key"))
                };
                let Some(proposals_public_key) = proposals_public_key.as_str() else {
                    return Err(Error::ParseFailed("Invalid proposals public key: Not a string"))
                };
                let Ok(proposals_public_key) = PublicKey::from_str(proposals_public_key) else {
                    return Err(Error::ParseFailed("Invalid proposals public key: Decoding failed"))
                };
                proposals_public_key
            }
        };

        let votes_secret_key = match table.get("votes_secret_key") {
            Some(votes_secret_key) => {
                let Some(votes_secret_key) = votes_secret_key.as_str() else {
                    return Err(Error::ParseFailed("Invalid votes secret key: Not a string"))
                };
                let Ok(votes_secret_key) = SecretKey::from_str(votes_secret_key) else {
                    return Err(Error::ParseFailed("Invalid votes secret key: Decoding failed"))
                };
                Some(votes_secret_key)
            }
            None => None,
        };
        let votes_public_key = match votes_secret_key {
            Some(votes_secret_key) => PublicKey::from_secret(votes_secret_key),
            None => {
                let Some(votes_public_key) = table.get("votes_public_key") else {
                    return Err(Error::ParseFailed("TOML does not contain votes public key"))
                };
                let Some(votes_public_key) = votes_public_key.as_str() else {
                    return Err(Error::ParseFailed("Invalid votes public key: Not a string"))
                };
                let Ok(votes_public_key) = PublicKey::from_str(votes_public_key) else {
                    return Err(Error::ParseFailed("Invalid votes public key: Decoding failed"))
                };
                votes_public_key
            }
        };

        let exec_secret_key = match table.get("exec_secret_key") {
            Some(exec_secret_key) => {
                let Some(exec_secret_key) = exec_secret_key.as_str() else {
                    return Err(Error::ParseFailed("Invalid exec secret key: Not a string"))
                };
                let Ok(exec_secret_key) = SecretKey::from_str(exec_secret_key) else {
                    return Err(Error::ParseFailed("Invalid exec secret key: Decoding failed"))
                };
                Some(exec_secret_key)
            }
            None => None,
        };
        let exec_public_key = match exec_secret_key {
            Some(exec_secret_key) => PublicKey::from_secret(exec_secret_key),
            None => {
                let Some(exec_public_key) = table.get("exec_public_key") else {
                    return Err(Error::ParseFailed("TOML does not contain exec public key"))
                };
                let Some(exec_public_key) = exec_public_key.as_str() else {
                    return Err(Error::ParseFailed("Invalid exec public key: Not a string"))
                };
                let Ok(exec_public_key) = PublicKey::from_str(exec_public_key) else {
                    return Err(Error::ParseFailed("Invalid exec public key: Decoding failed"))
                };
                exec_public_key
            }
        };

        let early_exec_secret_key = match table.get("early_exec_secret_key") {
            Some(early_exec_secret_key) => {
                let Some(early_exec_secret_key) = early_exec_secret_key.as_str() else {
                    return Err(Error::ParseFailed("Invalid early exec secret key: Not a string"))
                };
                let Ok(early_exec_secret_key) = SecretKey::from_str(early_exec_secret_key) else {
                    return Err(Error::ParseFailed("Invalid early exec secret key: Decoding failed"))
                };
                Some(early_exec_secret_key)
            }
            None => None,
        };
        let early_exec_public_key = match early_exec_secret_key {
            Some(early_exec_secret_key) => PublicKey::from_secret(early_exec_secret_key),
            None => {
                let Some(early_exec_public_key) = table.get("early_exec_public_key") else {
                    return Err(Error::ParseFailed("TOML does not contain early exec public key"))
                };
                let Some(early_exec_public_key) = early_exec_public_key.as_str() else {
                    return Err(Error::ParseFailed("Invalid early exec public key: Not a string"))
                };
                let Ok(early_exec_public_key) = PublicKey::from_str(early_exec_public_key) else {
                    return Err(Error::ParseFailed("Invalid early exec public key: Decoding failed"))
                };
                early_exec_public_key
            }
        };

        Ok(Self::new(
            proposer_limit,
            quorum,
            early_exec_quorum,
            approval_ratio_base,
            approval_ratio_quot,
            gov_token_id,
            notes_secret_key,
            notes_public_key,
            proposer_secret_key,
            proposer_public_key,
            proposals_secret_key,
            proposals_public_key,
            votes_secret_key,
            votes_public_key,
            exec_secret_key,
            exec_public_key,
            early_exec_secret_key,
            early_exec_public_key,
            bulla_blind,
        ))
    }

    /// Generate a toml string containing the DAO configuration.
    pub fn toml_str(&self) -> String {
        // Header comments
        let mut toml = String::from(
            "## DAO configuration file\n\
            ##\n\
            ## Please make sure you go through all the settings so you can configure\n\
            ## your DAO properly.\n\
            ##\n\
            ## If you want to restrict access to certain actions, the corresponding\n\
            ## secret key can be ommited. All public keys, along with the DAO configuration\n\
            ## parameters must be shared.\n\
            ##\n\
            ## If you want to combine access to certain actions, you can use the same\n\
            ## secret and public key combination for them.\n\n",
        );

        // Configuration parameters
        toml += &format!(
            "## ====== DAO configuration parameters =====\n\n\
            ## The minimum amount of governance tokens needed to open a proposal for this DAO\n\
            proposer_limit = \"{}\"\n\n\
            ## Minimal threshold of participating total tokens needed for a proposal to pass\n\
            quorum = \"{}\"\n\n\
            ## Minimal threshold of participating total tokens needed for a proposal to\n\
            ## be considered as strongly supported, enabling early execution.\n\
            ## Must be greater or equal to normal quorum.\n\
            early_exec_quorum = \"{}\"\n\n\
            ## The ratio of winning votes/total votes needed for a proposal to pass (2 decimals)\n\
            approval_ratio = {}\n\n\
            ## DAO's governance token ID\n\
            gov_token_id = \"{}\"\n\n\
            ## Bulla blind\n\
            bulla_blind = \"{}\"\n\n",
            encode_base10(self.dao.proposer_limit, BALANCE_BASE10_DECIMALS),
            encode_base10(self.dao.quorum, BALANCE_BASE10_DECIMALS),
            encode_base10(self.dao.early_exec_quorum, BALANCE_BASE10_DECIMALS),
            self.dao.approval_ratio_quot as f64 / self.dao.approval_ratio_base as f64,
            self.dao.gov_token_id,
            self.dao.bulla_blind,
        );

        // DAO actions keypairs
        toml += &format!(
            "## ====== DAO actions keypairs =====\n\n\
            ## DAO notes decryption keypair\n\
            notes_public_key = \"{}\"\n",
            self.dao.notes_public_key,
        );
        match self.notes_secret_key {
            Some(secret_key) => toml += &format!("notes_secret_key = \"{secret_key}\"\n\n"),
            None => toml += "\n",
        }
        toml += &format!(
            "## DAO proposals creator keypair\n\
            proposer_public_key = \"{}\"\n",
            self.dao.proposer_public_key,
        );
        match self.proposer_secret_key {
            Some(secret_key) => toml += &format!("proposer_secret_key = \"{secret_key}\"\n\n"),
            None => toml += "\n",
        }
        toml += &format!(
            "## DAO proposals viewer keypair\n\
            proposals_public_key = \"{}\"\n",
            self.dao.proposals_public_key,
        );
        match self.proposals_secret_key {
            Some(secret_key) => toml += &format!("proposals_secret_key = \"{secret_key}\"\n\n"),
            None => toml += "\n",
        }
        toml += &format!(
            "## DAO votes viewer keypair\n\
            votes_public_key = \"{}\"\n",
            self.dao.votes_public_key,
        );
        match self.votes_secret_key {
            Some(secret_key) => toml += &format!("votes_secret_key = \"{secret_key}\"\n\n"),
            None => toml += "\n",
        }
        toml += &format!(
            "## DAO proposals executor keypair\n\
            exec_public_key = \"{}\"\n",
            self.dao.exec_public_key,
        );
        match self.exec_secret_key {
            Some(secret_key) => toml += &format!("exec_secret_key = \"{secret_key}\"\n\n"),
            None => toml += "\n",
        }
        toml += &format!(
            "## DAO strongly supported proposals executor keypair\n\
            early_exec_public_key = \"{}\"",
            self.dao.early_exec_public_key,
        );
        if let Some(secret_key) = self.early_exec_secret_key {
            toml += &format!("\nearly_exec_secret_key = \"{secret_key}\"")
        }

        toml
    }
}

impl fmt::Display for DaoParams {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Grab known secret keys
        let notes_secret_key = match self.notes_secret_key {
            Some(secret_key) => format!("{secret_key}"),
            None => "None".to_string(),
        };
        let proposer_secret_key = match self.proposer_secret_key {
            Some(secret_key) => format!("{secret_key}"),
            None => "None".to_string(),
        };
        let proposals_secret_key = match self.proposals_secret_key {
            Some(secret_key) => format!("{secret_key}"),
            None => "None".to_string(),
        };
        let votes_secret_key = match self.votes_secret_key {
            Some(secret_key) => format!("{secret_key}"),
            None => "None".to_string(),
        };
        let exec_secret_key = match self.exec_secret_key {
            Some(secret_key) => format!("{secret_key}"),
            None => "None".to_string(),
        };
        let early_exec_secret_key = match self.early_exec_secret_key {
            Some(secret_key) => format!("{secret_key}"),
            None => "None".to_string(),
        };

        let s = format!(
            "{}\n{}\n{}: {} ({})\n{}: {} ({})\n{}: {} ({})\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}",
            "DAO Parameters",
            "==============",
            "Proposer limit",
            encode_base10(self.dao.proposer_limit, BALANCE_BASE10_DECIMALS),
            self.dao.proposer_limit,
            "Quorum",
            encode_base10(self.dao.quorum, BALANCE_BASE10_DECIMALS),
            self.dao.quorum,
            "Early Exec Quorum",
            encode_base10(self.dao.early_exec_quorum, BALANCE_BASE10_DECIMALS),
            self.dao.early_exec_quorum,
            "Approval ratio",
            self.dao.approval_ratio_quot as f64 / self.dao.approval_ratio_base as f64,
            "Governance Token ID",
            self.dao.gov_token_id,
            "Notes Public key",
            self.dao.notes_public_key,
            "Notes Secret key",
            notes_secret_key,
            "Proposer Public key",
            self.dao.proposer_public_key,
            "Proposer Secret key",
            proposer_secret_key,
            "Proposals Public key",
            self.dao.proposals_public_key,
            "Proposals Secret key",
            proposals_secret_key,
            "Votes Public key",
            self.dao.votes_public_key,
            "Votes Secret key",
            votes_secret_key,
            "Exec Public key",
            self.dao.exec_public_key,
            "Exec Secret key",
            exec_secret_key,
            "Early Exec Public key",
            self.dao.early_exec_public_key,
            "Early Exec Secret key",
            early_exec_secret_key,
            "Bulla blind",
            self.dao.bulla_blind,
        );

        write!(f, "{s}")
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
}

impl fmt::Display for DaoRecord {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Grab known secret keys
        let notes_secret_key = match self.params.notes_secret_key {
            Some(secret_key) => format!("{secret_key}"),
            None => "None".to_string(),
        };
        let proposer_secret_key = match self.params.proposer_secret_key {
            Some(secret_key) => format!("{secret_key}"),
            None => "None".to_string(),
        };
        let proposals_secret_key = match self.params.proposals_secret_key {
            Some(secret_key) => format!("{secret_key}"),
            None => "None".to_string(),
        };
        let votes_secret_key = match self.params.votes_secret_key {
            Some(secret_key) => format!("{secret_key}"),
            None => "None".to_string(),
        };
        let exec_secret_key = match self.params.exec_secret_key {
            Some(secret_key) => format!("{secret_key}"),
            None => "None".to_string(),
        };
        let early_exec_secret_key = match self.params.early_exec_secret_key {
            Some(secret_key) => format!("{secret_key}"),
            None => "None".to_string(),
        };

        // Grab mint information
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
            "{}\n{}\n{}: {}\n{}: {}\n{}: {} ({})\n{}: {} ({})\n{}: {} ({})\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}",
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
            "Early Exec Quorum",
            encode_base10(self.params.dao.early_exec_quorum, BALANCE_BASE10_DECIMALS),
            self.params.dao.early_exec_quorum,
            "Approval ratio",
            self.params.dao.approval_ratio_quot as f64 / self.params.dao.approval_ratio_base as f64,
            "Governance Token ID",
            self.params.dao.gov_token_id,
            "Notes Public key",
            self.params.dao.notes_public_key,
            "Notes Secret key",
            notes_secret_key,
            "Proposer Public key",
            self.params.dao.proposer_public_key,
            "Proposer Secret key",
            proposer_secret_key,
            "Proposals Public key",
            self.params.dao.proposals_public_key,
            "Proposals Secret key",
            proposals_secret_key,
            "Votes Public key",
            self.params.dao.votes_public_key,
            "Votes Secret key",
            votes_secret_key,
            "Exec Public key",
            self.params.dao.exec_public_key,
            "Exec Secret key",
            exec_secret_key,
            "Early Exec Public key",
            self.params.dao.early_exec_public_key,
            "Early Exec Secret key",
            early_exec_secret_key,
            "Bulla blind",
            self.params.dao.bulla_blind,
            "Leaf position",
            leaf_position,
            "Transaction hash",
            tx_hash,
            "Call index",
            call_index,
        );

        write!(f, "{s}")
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
    /// Block height of the transaction this proposal was deployed
    pub mint_height: Option<u32>,
    /// The transaction hash where the proposal was deployed
    pub tx_hash: Option<TransactionHash>,
    /// The call index in the transaction where the proposal was deployed
    pub call_index: Option<u8>,
    /// Block height of the transaction this proposal was executed
    pub exec_height: Option<u32>,
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
        let mint_height = match self.mint_height {
            Some(h) => format!("{h}"),
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
            "{}\n{}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {} ({})",
            "Proposal parameters",
            "===================",
            "Bulla",
            self.bulla(),
            "DAO Bulla",
            self.proposal.dao_bulla,
            "Proposal leaf position",
            leaf_position,
            "Proposal mint height",
            mint_height,
            "Proposal transaction hash",
            tx_hash,
            "Proposal call index",
            call_index,
            "Creation block window",
            self.proposal.creation_blockwindow,
            "Duration",
            self.proposal.duration_blockwindows,
            "Block windows"
        );

        write!(f, "{s}")
    }
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
    /// Block height of the transaction this vote was casted
    pub block_height: u32,
    /// Transaction hash where this vote was casted
    pub tx_hash: TransactionHash,
    /// Call index in the transaction where this vote was casted
    pub call_index: u8,
    /// Vote input nullifiers
    pub nullifiers: Vec<Nullifier>,
}

impl Drk {
    /// Initialize wallet with tables for the DAO contract.
    pub async fn initialize_dao(&self) -> WalletDbResult<()> {
        // Initialize DAO wallet schema
        let wallet_schema = include_str!("../dao.sql");
        self.wallet.exec_batch_sql(wallet_schema)?;

        Ok(())
    }

    /// Fetch DAO Merkle trees from the wallet.
    /// If a tree doesn't exists a new Merkle Tree is returned.
    pub async fn get_dao_trees(&self) -> Result<(MerkleTree, MerkleTree)> {
        let daos_tree = match self.cache.merkle_trees.get(SLED_MERKLE_TREES_DAO_DAOS)? {
            Some(tree_bytes) => deserialize_async(&tree_bytes).await?,
            None => MerkleTree::new(1),
        };
        let proposals_tree = match self.cache.merkle_trees.get(SLED_MERKLE_TREES_DAO_PROPOSALS)? {
            Some(tree_bytes) => deserialize_async(&tree_bytes).await?,
            None => MerkleTree::new(1),
        };
        Ok((daos_tree, proposals_tree))
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
                return Err(Error::DatabaseError(format!("[get_daos] DAOs retrieval failed: {e:?}")))
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
            Value::Blob(ref data_bytes) => Some(data_bytes.clone()),
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

        let mint_height = match row[7] {
            Value::Integer(mint_height) => {
                let Ok(mint_height) = u32::try_from(mint_height) else {
                    return Err(Error::ParseFailed("[get_dao_proposals] Mint height parsing failed"))
                };
                Some(mint_height)
            }
            Value::Null => None,
            _ => return Err(Error::ParseFailed("[get_dao_proposals] Mint height parsing failed")),
        };

        let tx_hash = match row[8] {
            Value::Blob(ref tx_hash_bytes) => Some(deserialize_async(tx_hash_bytes).await?),
            Value::Null => None,
            _ => {
                return Err(Error::ParseFailed(
                    "[get_dao_proposals] Transaction hash bytes parsing failed",
                ))
            }
        };

        let call_index = match row[9] {
            Value::Integer(call_index) => {
                let Ok(call_index) = u8::try_from(call_index) else {
                    return Err(Error::ParseFailed("[get_dao_proposals] Call index parsing failed"))
                };
                Some(call_index)
            }
            Value::Null => None,
            _ => return Err(Error::ParseFailed("[get_dao_proposals] Call index parsing failed")),
        };

        let exec_height = match row[10] {
            Value::Integer(exec_height) => {
                let Ok(exec_height) = u32::try_from(exec_height) else {
                    return Err(Error::ParseFailed(
                        "[get_dao_proposals] Execution height parsing failed",
                    ))
                };
                Some(exec_height)
            }
            Value::Null => None,
            _ => {
                return Err(Error::ParseFailed(
                    "[get_dao_proposals] Execution height parsing failed",
                ))
            }
        };

        let exec_tx_hash = match row[11] {
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
            mint_height,
            tx_hash,
            call_index,
            exec_height,
            exec_tx_hash,
        })
    }

    /// Fetch all known DAO proposals from the wallet given a DAO name.
    pub async fn get_dao_proposals(&self, name: &str) -> Result<Vec<ProposalRecord>> {
        let Ok(dao) = self.get_dao_by_name(name).await else {
            return Err(Error::DatabaseError(format!(
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
                return Err(Error::DatabaseError(format!(
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

    /// Auxiliary function to apply `DaoFunction::Mint` call data to
    /// the wallet and update the provided scan cache.
    /// Returns a flag indicating if the provided call refers to our
    /// own wallet.
    async fn apply_dao_mint_data(
        &self,
        scan_cache: &mut ScanCache,
        new_bulla: &DaoBulla,
        tx_hash: &TransactionHash,
        call_index: &u8,
    ) -> Result<bool> {
        // Append the new dao bulla to the Merkle tree.
        // Every dao bulla has to be added.
        scan_cache.dao_daos_tree.append(MerkleNode::from(new_bulla.inner()));

        // Check if we have the DAO
        if !scan_cache.own_daos.contains_key(new_bulla) {
            return Ok(false)
        }

        // Confirm it
        println!(
            "[apply_dao_mint_data] Found minted DAO {new_bulla}, noting down for wallet update"
        );
        if let Err(e) = self
            .confirm_dao(new_bulla, &scan_cache.dao_daos_tree.mark().unwrap(), tx_hash, call_index)
            .await
        {
            return Err(Error::DatabaseError(format!(
                "[apply_dao_mint_data] Confirm DAO failed: {e:?}"
            )))
        }

        Ok(true)
    }

    /// Auxiliary function to apply `DaoFunction::Propose` call data to
    /// the wallet and update the provided scan cache.
    /// Returns a flag indicating if the provided call refers to our
    /// own wallet.
    async fn apply_dao_propose_data(
        &self,
        scan_cache: &mut ScanCache,
        params: &DaoProposeParams,
        tx_hash: &TransactionHash,
        call_index: &u8,
        mint_height: &u32,
    ) -> Result<bool> {
        // Append the new proposal bulla to the Merkle tree.
        // Every proposal bulla has to be added.
        scan_cache.dao_proposals_tree.append(MerkleNode::from(params.proposal_bulla.inner()));

        // If we're able to decrypt this note, that's the way to link it
        // to a specific DAO.
        for (dao, (proposals_secret_key, _)) in &scan_cache.own_daos {
            // Check if we have the proposals key
            let Some(proposals_secret_key) = proposals_secret_key else { continue };

            // Try to decrypt the proposal note
            let Ok(note) = params.note.decrypt::<DaoProposal>(proposals_secret_key) else {
                continue
            };

            // We managed to decrypt it. Let's place this in a proper ProposalRecord object
            println!("[apply_dao_propose_data] Managed to decrypt proposal note for DAO: {dao}");

            // Check if we already got the record
            let our_proposal = if scan_cache.own_proposals.contains_key(&params.proposal_bulla) {
                // Grab the record from the db
                let mut our_proposal =
                    self.get_dao_proposal_by_bulla(&params.proposal_bulla).await?;
                our_proposal.leaf_position = scan_cache.dao_proposals_tree.mark();
                our_proposal.money_snapshot_tree = Some(scan_cache.money_tree.clone());
                our_proposal.nullifiers_smt_snapshot = Some(scan_cache.money_smt.store.snapshot()?);
                our_proposal.mint_height = Some(*mint_height);
                our_proposal.tx_hash = Some(*tx_hash);
                our_proposal.call_index = Some(*call_index);
                our_proposal
            } else {
                let our_proposal = ProposalRecord {
                    proposal: note,
                    data: None,
                    leaf_position: scan_cache.dao_proposals_tree.mark(),
                    money_snapshot_tree: Some(scan_cache.money_tree.clone()),
                    nullifiers_smt_snapshot: Some(scan_cache.money_smt.store.snapshot()?),
                    mint_height: Some(*mint_height),
                    tx_hash: Some(*tx_hash),
                    call_index: Some(*call_index),
                    exec_height: None,
                    exec_tx_hash: None,
                };
                scan_cache.own_proposals.insert(params.proposal_bulla, *dao);
                our_proposal
            };

            // Update/store our record
            if let Err(e) = self.put_dao_proposal(&our_proposal).await {
                return Err(Error::DatabaseError(format!(
                    "[apply_dao_propose_data] Put DAO proposals failed: {e:?}"
                )))
            }

            return Ok(true)
        }

        Ok(false)
    }

    /// Auxiliary function to apply `DaoFunction::Vote` call data to
    /// the wallet.
    /// Returns a flag indicating if the provided call refers to our
    /// own wallet.
    async fn apply_dao_vote_data(
        &self,
        scan_cache: &ScanCache,
        params: &DaoVoteParams,
        tx_hash: &TransactionHash,
        call_index: &u8,
        block_height: &u32,
    ) -> Result<bool> {
        // Check if we got the corresponding proposal
        let Some(dao_bulla) = scan_cache.own_proposals.get(&params.proposal_bulla) else {
            return Ok(false)
        };

        // Grab the proposal DAO votes key
        let Some((_, votes_secret_key)) = scan_cache.own_daos.get(dao_bulla) else {
            return Err(Error::DatabaseError(format!(
                "[apply_dao_vote_data] Couldn't find proposal {} DAO {}",
                params.proposal_bulla, dao_bulla,
            )))
        };

        // Check if we actually have the votes key
        let Some(votes_secret_key) = votes_secret_key else { return Ok(false) };

        // Decrypt the vote note
        let note = match params.note.decrypt_unsafe(votes_secret_key) {
            Ok(n) => n,
            Err(e) => {
                return Err(Error::DatabaseError(format!(
                    "[apply_dao_vote_data] Couldn't decrypt proposal {} vote with DAO {} keys: {e}",
                    params.proposal_bulla, dao_bulla,
                )))
            }
        };

        // Create the DAO vote record
        let vote_option = fp_to_u64(note[0]).unwrap();
        if vote_option > 1 {
            return Err(Error::DatabaseError(format!(
                "[apply_dao_vote_data] Malformed vote for proposal {}: {vote_option}",
                params.proposal_bulla,
            )))
        }
        let vote_option = vote_option != 0;
        let yes_vote_blind = Blind(fp_mod_fv(note[1]));
        let all_vote_value = fp_to_u64(note[2]).unwrap();
        let all_vote_blind = Blind(fp_mod_fv(note[3]));

        let v = VoteRecord {
            id: 0, // This will be set by SQLite AUTOINCREMENT
            proposal: params.proposal_bulla,
            vote_option,
            yes_vote_blind,
            all_vote_value,
            all_vote_blind,
            block_height: *block_height,
            tx_hash: *tx_hash,
            call_index: *call_index,
            nullifiers: params.inputs.iter().map(|i| i.vote_nullifier).collect(),
        };

        if let Err(e) = self.put_dao_vote(&v).await {
            return Err(Error::DatabaseError(format!(
                "[apply_dao_vote_data] Put DAO votes failed: {e:?}"
            )))
        }

        Ok(true)
    }

    /// Auxiliary function to apply `DaoFunction::Exec` call data to
    /// the wallet and update the provided scan cache.
    /// Returns a flag indicating if the provided call refers to our
    /// own wallet.
    async fn apply_dao_exec_data(
        &self,
        scan_cache: &ScanCache,
        params: &DaoExecParams,
        tx_hash: &TransactionHash,
        exec_height: &u32,
    ) -> Result<bool> {
        // Check if we got the corresponding proposal
        if !scan_cache.own_proposals.contains_key(&params.proposal_bulla) {
            return Ok(false)
        }

        // Grab proposal record key
        let key = serialize_async(&params.proposal_bulla).await;

        // Create an SQL `UPDATE` query to update proposal exec transaction hash
        let query = format!(
            "UPDATE {} SET {} = ?1, {} = ?2 WHERE {} = ?3;",
            *DAO_PROPOSALS_TABLE,
            DAO_PROPOSALS_COL_EXEC_HEIGHT,
            DAO_PROPOSALS_COL_EXEC_TX_HASH,
            DAO_PROPOSALS_COL_BULLA,
        );

        // Execute the query
        if let Err(e) = self.wallet.exec_sql(
            &query,
            rusqlite::params![Some(*exec_height), Some(serialize_async(tx_hash).await), key],
        ) {
            return Err(Error::DatabaseError(format!(
                "[apply_dao_exec_data] Update DAO proposal failed: {e:?}"
            )))
        }

        Ok(true)
    }

    /// Append data related to DAO contract transactions into the
    /// wallet database and update the provided scan cache.
    /// Returns a flag indicating if the daos tree should be updated,
    /// one indicating if the proposals tree should be updated and
    /// another one indicating if provided data refer to our own
    /// wallet.
    pub async fn apply_tx_dao_data(
        &self,
        scan_cache: &mut ScanCache,
        data: &[u8],
        tx_hash: &TransactionHash,
        call_idx: &u8,
        block_height: &u32,
    ) -> Result<(bool, bool, bool)> {
        // Run through the transaction call data and see what we got:
        match DaoFunction::try_from(data[0])? {
            DaoFunction::Mint => {
                println!("[apply_tx_dao_data] Found Dao::Mint call");
                let params: DaoMintParams = deserialize_async(&data[1..]).await?;
                let own_tx = self
                    .apply_dao_mint_data(scan_cache, &params.dao_bulla, tx_hash, call_idx)
                    .await?;
                Ok((true, false, own_tx))
            }
            DaoFunction::Propose => {
                println!("[apply_tx_dao_data] Found Dao::Propose call");
                let params: DaoProposeParams = deserialize_async(&data[1..]).await?;
                let own_tx = self
                    .apply_dao_propose_data(scan_cache, &params, tx_hash, call_idx, block_height)
                    .await?;
                Ok((false, true, own_tx))
            }
            DaoFunction::Vote => {
                println!("[apply_tx_dao_data] Found Dao::Vote call");
                let params: DaoVoteParams = deserialize_async(&data[1..]).await?;
                let own_tx = self
                    .apply_dao_vote_data(scan_cache, &params, tx_hash, call_idx, block_height)
                    .await?;
                Ok((false, false, own_tx))
            }
            DaoFunction::Exec => {
                println!("[apply_tx_dao_data] Found Dao::Exec call");
                let params: DaoExecParams = deserialize_async(&data[1..]).await?;
                let own_tx =
                    self.apply_dao_exec_data(scan_cache, &params, tx_hash, block_height).await?;
                Ok((false, false, own_tx))
            }
            DaoFunction::AuthMoneyTransfer => {
                println!("[apply_tx_dao_data] Found Dao::AuthMoneyTransfer call");
                // Does nothing, just verifies the other calls are correct
                Ok((false, false, false))
            }
        }
    }

    /// Confirm already imported DAO metadata into the wallet.
    /// Here we just write the leaf position, tx hash, and call index.
    /// Panics if the fields are None.
    pub async fn confirm_dao(
        &self,
        dao: &DaoBulla,
        leaf_position: &bridgetree::Position,
        tx_hash: &TransactionHash,
        call_index: &u8,
    ) -> WalletDbResult<()> {
        // Grab dao record key
        let key = serialize_async(dao).await;

        // Create an SQL `UPDATE` query
        let query = format!(
            "UPDATE {} SET {} = ?1, {} = ?2, {} = ?3 WHERE {} = ?4;",
            *DAO_DAOS_TABLE,
            DAO_DAOS_COL_LEAF_POSITION,
            DAO_DAOS_COL_TX_HASH,
            DAO_DAOS_COL_CALL_INDEX,
            DAO_DAOS_COL_BULLA
        );

        // Create its params
        let params = rusqlite::params![
            serialize_async(leaf_position).await,
            serialize_async(tx_hash).await,
            call_index,
            key,
        ];

        // Execute the query
        self.wallet.exec_sql(&query, params)
    }

    /// Import given DAO proposal into the wallet.
    pub async fn put_dao_proposal(&self, proposal: &ProposalRecord) -> Result<()> {
        // Check that we already have the proposal DAO
        if let Err(e) = self.get_dao_by_bulla(&proposal.proposal.dao_bulla).await {
            return Err(Error::DatabaseError(format!(
                "[put_dao_proposal] Couldn't find proposal {} DAO {}: {e}",
                proposal.bulla(),
                proposal.proposal.dao_bulla
            )))
        }

        // Grab proposal record key
        let key = serialize_async(&proposal.bulla()).await;

        // Create an SQL `INSERT OR REPLACE` query
        let query = format!(
            "INSERT OR REPLACE INTO {} ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12);",
            *DAO_PROPOSALS_TABLE,
            DAO_PROPOSALS_COL_BULLA,
            DAO_PROPOSALS_COL_DAO_BULLA,
            DAO_PROPOSALS_COL_PROPOSAL,
            DAO_PROPOSALS_COL_DATA,
            DAO_PROPOSALS_COL_LEAF_POSITION,
            DAO_PROPOSALS_COL_MONEY_SNAPSHOT_TREE,
            DAO_PROPOSALS_COL_NULLIFIERS_SMT_SNAPSHOT,
            DAO_PROPOSALS_COL_MINT_HEIGHT,
            DAO_PROPOSALS_COL_TX_HASH,
            DAO_PROPOSALS_COL_CALL_INDEX,
            DAO_PROPOSALS_COL_EXEC_HEIGHT,
            DAO_PROPOSALS_COL_EXEC_TX_HASH,
        );

        // Create its params
        let data = match &proposal.data {
            Some(data) => Some(data),
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
            Some(nullifiers_smt_snapshot) => Some(serialize_async(nullifiers_smt_snapshot).await),
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

        let params = rusqlite::params![
            key,
            serialize_async(&proposal.proposal.dao_bulla).await,
            serialize_async(&proposal.proposal).await,
            data,
            leaf_position,
            money_snapshot_tree,
            nullifiers_smt_snapshot,
            proposal.mint_height,
            tx_hash,
            proposal.call_index,
            proposal.exec_height,
            exec_tx_hash,
        ];

        // Execute the query
        if let Err(e) = self.wallet.exec_sql(&query, params) {
            return Err(Error::DatabaseError(format!(
                "[put_dao_proposal] Proposal insert failed: {e:?}"
            )))
        }

        Ok(())
    }

    /// Unconfirm imported DAO proposals by removing the leaf position, tx hash, and call index.
    pub async fn unconfirm_proposals(&self, proposals: &[ProposalRecord]) -> WalletDbResult<()> {
        for proposal in proposals {
            let query = format!(
                "UPDATE {} SET {} = NULL, {} = NULL, {} = NULL, {} = NULL, {} = NULL, {} = NULL, {} = NULL WHERE {} = ?1;",
                *DAO_PROPOSALS_TABLE,
                DAO_PROPOSALS_COL_LEAF_POSITION,
                DAO_PROPOSALS_COL_MONEY_SNAPSHOT_TREE,
                DAO_PROPOSALS_COL_NULLIFIERS_SMT_SNAPSHOT,
                DAO_PROPOSALS_COL_MINT_HEIGHT,
                DAO_PROPOSALS_COL_TX_HASH,
                DAO_PROPOSALS_COL_CALL_INDEX,
                DAO_PROPOSALS_COL_EXEC_TX_HASH,
                DAO_PROPOSALS_COL_BULLA
            );
            self.wallet
                .exec_sql(&query, rusqlite::params![serialize_async(&proposal.bulla()).await])?;
        }

        Ok(())
    }

    /// Import given DAO vote into the wallet.
    pub async fn put_dao_vote(&self, vote: &VoteRecord) -> WalletDbResult<()> {
        println!("Importing DAO vote into wallet");

        // Create an SQL `INSERT OR REPLACE` query
        let query = format!(
            "INSERT INTO {} ({}, {}, {}, {}, {}, {}, {}, {}, {}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9);",
            *DAO_VOTES_TABLE,
            DAO_VOTES_COL_PROPOSAL_BULLA,
            DAO_VOTES_COL_VOTE_OPTION,
            DAO_VOTES_COL_YES_VOTE_BLIND,
            DAO_VOTES_COL_ALL_VOTE_VALUE,
            DAO_VOTES_COL_ALL_VOTE_BLIND,
            DAO_VOTES_COL_BLOCK_HEIGHT,
            DAO_VOTES_COL_TX_HASH,
            DAO_VOTES_COL_CALL_INDEX,
            DAO_VOTES_COL_NULLIFIERS,
        );

        // Create its params
        let params = rusqlite::params![
            serialize_async(&vote.proposal).await,
            vote.vote_option as u64,
            serialize_async(&vote.yes_vote_blind).await,
            serialize_async(&vote.all_vote_value).await,
            serialize_async(&vote.all_vote_blind).await,
            vote.block_height,
            serialize_async(&vote.tx_hash).await,
            vote.call_index,
            serialize_async(&vote.nullifiers).await,
        ];

        // Execute the query
        self.wallet.exec_sql(&query, params)?;

        println!("DAO vote added to wallet");

        Ok(())
    }

    /// Reset the DAO Merkle trees in the cache.
    pub async fn reset_dao_trees(&self) -> WalletDbResult<()> {
        println!("Resetting DAO Merkle trees");
        if let Err(e) = self.cache.merkle_trees.remove(SLED_MERKLE_TREES_DAO_DAOS) {
            println!("[reset_dao_trees] Resetting DAO DAOs Merkle tree failed: {e:?}");
            return Err(WalletDbError::GenericError)
        }
        if let Err(e) = self.cache.merkle_trees.remove(SLED_MERKLE_TREES_DAO_PROPOSALS) {
            println!("[reset_dao_trees] Resetting DAO Proposals Merkle tree failed: {e:?}");
            return Err(WalletDbError::GenericError)
        }
        println!("Successfully reset DAO Merkle trees");

        Ok(())
    }

    /// Reset confirmed DAOs in the wallet.
    pub async fn reset_daos(&self) -> WalletDbResult<()> {
        println!("Resetting DAO confirmations");
        let query = format!(
            "UPDATE {} SET {} = NULL, {} = NULL, {} = NULL;",
            *DAO_DAOS_TABLE,
            DAO_DAOS_COL_LEAF_POSITION,
            DAO_DAOS_COL_TX_HASH,
            DAO_DAOS_COL_CALL_INDEX,
        );
        self.wallet.exec_sql(&query, &[])?;
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
    pub async fn import_dao(&self, name: &str, params: &DaoParams) -> Result<()> {
        // First let's check if we've imported this DAO with the given name before.
        if self.get_dao_by_name(name).await.is_ok() {
            return Err(Error::DatabaseError(
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
                serialize_async(params).await,
            ],
        ) {
            return Err(Error::DatabaseError(format!("[import_dao] DAO insert failed: {e:?}")))
        };

        Ok(())
    }

    /// Update given DAO params into the wallet, if the corresponding DAO exists.
    pub async fn update_dao_keys(&self, params: &DaoParams) -> Result<()> {
        // Grab the params DAO
        let bulla = params.dao.to_bulla();
        let Ok(dao) = self.get_dao_by_bulla(&bulla).await else {
            return Err(Error::DatabaseError(format!("[import_dao] DAO {bulla} was not found")))
        };

        println!("Updating \"{}\" DAO keys into the wallet", dao.name);

        let query = format!(
            "UPDATE {} SET {} = ?1 WHERE {} = ?2;",
            *DAO_DAOS_TABLE, DAO_DAOS_COL_PARAMS, DAO_DAOS_COL_BULLA,
        );
        if let Err(e) = self.wallet.exec_sql(
            &query,
            rusqlite::params![serialize_async(params).await, serialize_async(&bulla).await,],
        ) {
            return Err(Error::DatabaseError(format!("[update_dao_keys] DAO update failed: {e:?}")))
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
                return Err(Error::DatabaseError(format!(
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
                return Err(Error::DatabaseError(format!(
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
                return Err(Error::DatabaseError(format!(
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
                return Err(Error::DatabaseError(format!(
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
                return Err(Error::DatabaseError(format!(
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

            let Value::Integer(block_height) = row[6] else {
                return Err(Error::ParseFailed(
                    "[get_dao_proposal_votes] Block height parsing failed",
                ))
            };
            let Ok(block_height) = u32::try_from(block_height) else {
                return Err(Error::ParseFailed(
                    "[get_dao_proposal_votes] Block height parsing failed",
                ))
            };

            let Value::Blob(ref tx_hash_bytes) = row[7] else {
                return Err(Error::ParseFailed(
                    "[get_dao_proposal_votes] Transaction hash bytes parsing failed",
                ))
            };
            let tx_hash = deserialize_async(tx_hash_bytes).await?;

            let Value::Integer(call_index) = row[8] else {
                return Err(Error::ParseFailed("[get_dao_proposal_votes] Call index parsing failed"))
            };
            let Ok(call_index) = u8::try_from(call_index) else {
                return Err(Error::ParseFailed("[get_dao_proposal_votes] Call index parsing failed"))
            };

            let Value::Blob(ref nullifiers_bytes) = row[9] else {
                return Err(Error::ParseFailed(
                    "[get_dao_proposal_votes] Nullifiers bytes parsing failed",
                ))
            };
            let nullifiers = deserialize_async(nullifiers_bytes).await?;

            let vote = VoteRecord {
                id,
                proposal,
                vote_option,
                yes_vote_blind,
                all_vote_value,
                all_vote_blind,
                block_height,
                tx_hash,
                call_index,
                nullifiers,
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

        // Check that we have all the keys
        if dao.params.notes_secret_key.is_none() ||
            dao.params.proposer_secret_key.is_none() ||
            dao.params.proposals_secret_key.is_none() ||
            dao.params.votes_secret_key.is_none() ||
            dao.params.exec_secret_key.is_none() ||
            dao.params.early_exec_secret_key.is_none()
        {
            return Err(Error::Custom(
                "[dao_mint] We need all the secrets key to mint the DAO on-chain".to_string(),
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
            return Err(Error::DatabaseError("[dao_mint] DAO Mint circuit not found".to_string()))
        };

        let dao_mint_zkbin = ZkBinary::decode(&dao_mint_zkbin.1)?;

        let dao_mint_circuit = ZkCircuit::new(empty_witnesses(&dao_mint_zkbin)?, &dao_mint_zkbin);

        // Creating DAO Mint circuit proving key
        let dao_mint_pk = ProvingKey::build(dao_mint_zkbin.k, &dao_mint_circuit);

        // Create the DAO mint call
        let notes_secret_key = dao.params.notes_secret_key.unwrap();
        let (params, proofs) = make_mint_call(
            &dao.params.dao,
            &notes_secret_key,
            &dao.params.proposer_secret_key.unwrap(),
            &dao.params.proposals_secret_key.unwrap(),
            &dao.params.votes_secret_key.unwrap(),
            &dao.params.exec_secret_key.unwrap(),
            &dao.params.early_exec_secret_key.unwrap(),
            &dao_mint_zkbin,
            &dao_mint_pk,
        )?;
        let mut data = vec![DaoFunction::Mint as u8];
        params.encode_async(&mut data).await?;
        let call = ContractCall { contract_id: *DAO_CONTRACT_ID, data };

        // Create the TransactionBuilder containing above call
        let mut tx_builder = TransactionBuilder::new(ContractCallLeaf { call, proofs }, vec![])?;

        // We first have to execute the fee-less tx to gather its used gas, and then we feed
        // it into the fee-creating function.
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&[notes_secret_key])?;
        tx.signatures.push(sigs);

        let tree = self.get_money_tree().await?;
        let (fee_call, fee_proofs, fee_secrets) =
            self.append_fee_call(&tx, &tree, &fee_pk, &fee_zkbin, None).await?;

        // Append the fee call to the transaction
        tx_builder.append(ContractCallLeaf { call: fee_call, proofs: fee_proofs }, vec![])?;

        // Now build the actual transaction and sign it with all necessary keys.
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&[notes_secret_key])?;
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
        duration_blockwindows: u64,
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

        // Check that we have the proposer key
        if dao.params.proposer_secret_key.is_none() {
            return Err(Error::Custom(
                "[dao_propose_transfer] We need the proposer secret key to create proposals for this DAO".to_string(),
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
        let proposal_coinattrs = CoinAttributes {
            public_key: recipient,
            value: amount,
            token_id,
            spend_hook: spend_hook.unwrap_or(FuncId::none()),
            user_data: user_data.unwrap_or(pallas::Base::ZERO),
            blind: Blind::random(&mut OsRng),
        };

        // Convert coin_params to actual coins
        let proposal_coins = vec![proposal_coinattrs.to_coin()];
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

        // Retrieve next block height and current block time target,
        // to compute their window.
        let next_block_height = self.get_next_block_height().await?;
        let block_target = self.get_block_target().await?;
        let creation_blockwindow = blockwindow(next_block_height, block_target);

        // Create the actual proposal
        let proposal = DaoProposal {
            auth_calls,
            creation_blockwindow,
            duration_blockwindows,
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
            mint_height: None,
            tx_hash: None,
            call_index: None,
            exec_height: None,
            exec_tx_hash: None,
        };

        if let Err(e) = self.put_dao_proposal(&proposal_record).await {
            return Err(Error::DatabaseError(format!(
                "[dao_propose_transfer] Put DAO proposal failed: {e:?}"
            )))
        }

        Ok(proposal_record)
    }

    /// Create a DAO generic proposal.
    pub async fn dao_propose_generic(
        &self,
        name: &str,
        duration_blockwindows: u64,
        user_data: Option<pallas::Base>,
    ) -> Result<ProposalRecord> {
        // Fetch DAO and check its deployed
        let dao = self.get_dao_by_name(name).await?;
        if dao.leaf_position.is_none() || dao.tx_hash.is_none() || dao.call_index.is_none() {
            return Err(Error::Custom(
                "[dao_propose_generic] DAO seems to not have been deployed yet".to_string(),
            ))
        }

        // Check that we have the proposer key
        if dao.params.proposer_secret_key.is_none() {
            return Err(Error::Custom(
                "[dao_propose_generic] We need the proposer secret key to create proposals for this DAO".to_string(),
            ))
        }

        // Retrieve next block height and current block time target,
        // to compute their window.
        let next_block_height = self.get_next_block_height().await?;
        let block_target = self.get_block_target().await?;
        let creation_blockwindow = blockwindow(next_block_height, block_target);

        // Create the actual proposal
        let proposal = DaoProposal {
            auth_calls: vec![],
            creation_blockwindow,
            duration_blockwindows,
            user_data: user_data.unwrap_or(pallas::Base::ZERO),
            dao_bulla: dao.bulla(),
            blind: Blind::random(&mut OsRng),
        };

        let proposal_record = ProposalRecord {
            proposal,
            data: None,
            leaf_position: None,
            money_snapshot_tree: None,
            nullifiers_smt_snapshot: None,
            mint_height: None,
            tx_hash: None,
            call_index: None,
            exec_height: None,
            exec_tx_hash: None,
        };

        if let Err(e) = self.put_dao_proposal(&proposal_record).await {
            return Err(Error::DatabaseError(format!(
                "[dao_propose_generic] Put DAO proposal failed: {e:?}"
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
        let proposal_coinattrs: CoinAttributes =
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

        // Check that we have the proposer key
        if dao.params.proposer_secret_key.is_none() {
            return Err(Error::Custom(
                "[dao_transfer_proposal_tx] We need the proposer secret key to create proposals for this DAO".to_string(),
            ))
        }

        // Fetch DAO unspent OwnCoins to see what its balance is for the coin
        let dao_spend_hook =
            FuncRef { contract_id: *DAO_CONTRACT_ID, func_code: DaoFunction::Exec as u8 }
                .to_func_id();
        let dao_owncoins = self
            .get_contract_token_coins(
                &proposal_coinattrs.token_id,
                &dao_spend_hook,
                &proposal.proposal.dao_bulla.inner(),
            )
            .await?;
        if dao_owncoins.is_empty() {
            return Err(Error::Custom(format!(
                "[dao_transfer_proposal_tx] Did not find any {} unspent coins owned by this DAO",
                proposal_coinattrs.token_id,
            )))
        }

        // Check DAO balance is sufficient
        if dao_owncoins.iter().map(|x| x.note.value).sum::<u64>() < proposal_coinattrs.value {
            return Err(Error::Custom(format!(
                "[dao_transfer_proposal_tx] Not enough DAO balance for token ID: {}",
                proposal_coinattrs.token_id,
            )))
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
        let store = CacheSmtStorage::new(CacheOverlay::new(&self.cache)?, SLED_MONEY_SMT_TREE);
        let money_null_smt = CacheSmt::new(store, PoseidonFp::new(), &EMPTY_NODES_FP);

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
            &dao.params.proposer_secret_key.unwrap(),
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

    /// Create a DAO generic proposal transaction.
    pub async fn dao_generic_proposal_tx(&self, proposal: &ProposalRecord) -> Result<Transaction> {
        // Fetch DAO and check its deployed
        let Ok(dao) = self.get_dao_by_bulla(&proposal.proposal.dao_bulla).await else {
            return Err(Error::Custom(format!(
                "[dao_generic_proposal_tx] DAO {} was not found",
                proposal.proposal.dao_bulla
            )))
        };
        if dao.leaf_position.is_none() || dao.tx_hash.is_none() || dao.call_index.is_none() {
            return Err(Error::Custom(
                "[dao_generic_proposal_tx] DAO seems to not have been deployed yet".to_string(),
            ))
        }

        // Check that we have the proposer key
        if dao.params.proposer_secret_key.is_none() {
            return Err(Error::Custom(
                "[dao_generic_proposal_tx] We need the proposer secret key to create proposals for this DAO".to_string(),
            ))
        }

        // Fetch our own governance OwnCoins to see what our balance is
        let gov_owncoins = self.get_token_coins(&dao.params.dao.gov_token_id).await?;
        if gov_owncoins.is_empty() {
            return Err(Error::Custom(format!(
                "[dao_generic_proposal_tx] Did not find any governance {} coins in wallet",
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
                "[dao_generic_proposal_tx] Not enough gov token {} balance to propose",
                dao.params.dao.gov_token_id
            )))
        }

        // Now we need to do a lookup for the zkas proof bincodes, and create
        // the circuit objects and proving keys so we can build the transaction.
        // We also do this through the RPC. First we grab the fee call from money.
        let zkas_bins = self.lookup_zkas(&MONEY_CONTRACT_ID).await?;

        let Some(fee_zkbin) = zkas_bins.iter().find(|x| x.0 == MONEY_CONTRACT_ZKAS_FEE_NS_V1)
        else {
            return Err(Error::Custom("[dao_generic_proposal_tx] Fee circuit not found".to_string()))
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
                "[dao_generic_proposal_tx] Propose Burn circuit not found".to_string(),
            ))
        };

        let Some(propose_main_zkbin) =
            zkas_bins.iter().find(|x| x.0 == DAO_CONTRACT_ZKAS_DAO_PROPOSE_MAIN_NS)
        else {
            return Err(Error::Custom(
                "[dao_generic_proposal_tx] Propose Main circuit not found".to_string(),
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
        let store = CacheSmtStorage::new(CacheOverlay::new(&self.cache)?, SLED_MONEY_SMT_TREE);
        let money_null_smt = CacheSmt::new(store, PoseidonFp::new(), &EMPTY_NODES_FP);

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
            &dao.params.proposer_secret_key.unwrap(),
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
        proposal_bulla: &DaoProposalBulla,
        vote_option: bool,
        weight: Option<u64>,
    ) -> Result<Transaction> {
        // Feth the proposal and check its deployed
        let Ok(proposal) = self.get_dao_proposal_by_bulla(proposal_bulla).await else {
            return Err(Error::Custom(format!("[dao_vote] Proposal {proposal_bulla} was not found")))
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

        // Check proposal is not executed
        if let Some(exec_tx_hash) = proposal.exec_tx_hash {
            return Err(Error::Custom(format!(
                "[dao_vote] Proposal was executed on transaction: {exec_tx_hash}"
            )))
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

        // Fetch all the proposal votes to check for duplicate nullifiers
        let votes = self.get_dao_proposal_votes(proposal_bulla).await?;
        let mut votes_nullifiers = vec![];
        for vote in votes {
            for nullifier in vote.nullifiers {
                if !votes_nullifiers.contains(&nullifier) {
                    votes_nullifiers.push(nullifier);
                }
            }
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
            let nullifier = poseidon_hash([gov_owncoin.secret.inner(), gov_owncoin.coin.inner()]);
            let vote_nullifier =
                poseidon_hash([nullifier, gov_owncoin.secret.inner(), proposal_bulla.inner()]);
            if votes_nullifiers.contains(&vote_nullifier.into()) {
                return Err(Error::Custom("[dao_vote] Duplicate input nullifier found".to_string()))
            };

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

        // Retrieve next block height and current block time target,
        // to compute their window.
        let next_block_height = self.get_next_block_height().await?;
        let block_target = self.get_block_target().await?;
        let current_blockwindow = blockwindow(next_block_height, block_target);

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
            current_blockwindow,
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

    /// Execute a DAO transfer proposal.
    pub async fn dao_exec_transfer(
        &self,
        proposal: &ProposalRecord,
        early: bool,
    ) -> Result<Transaction> {
        if proposal.leaf_position.is_none() ||
            proposal.money_snapshot_tree.is_none() ||
            proposal.nullifiers_smt_snapshot.is_none() ||
            proposal.tx_hash.is_none() ||
            proposal.call_index.is_none()
        {
            return Err(Error::Custom(
                "[dao_exec_transfer] Proposal seems to not have been deployed yet".to_string(),
            ))
        }

        // Check proposal is not executed
        if let Some(exec_tx_hash) = proposal.exec_tx_hash {
            return Err(Error::Custom(format!(
                "[dao_exec_transfer] Proposal was executed on transaction: {exec_tx_hash}"
            )))
        }

        // Check we know the plaintext data and they are valid
        if proposal.data.is_none() {
            return Err(Error::Custom(
                "[dao_exec_transfer] Proposal plainext data is empty".to_string(),
            ))
        }
        let proposal_coinattrs: CoinAttributes =
            deserialize_async(proposal.data.as_ref().unwrap()).await?;

        // Fetch DAO and check its deployed
        let Ok(dao) = self.get_dao_by_bulla(&proposal.proposal.dao_bulla).await else {
            return Err(Error::Custom(format!(
                "[dao_exec_transfer] DAO {} was not found",
                proposal.proposal.dao_bulla
            )))
        };
        if dao.leaf_position.is_none() || dao.tx_hash.is_none() || dao.call_index.is_none() {
            return Err(Error::Custom(
                "[dao_exec_transfer] DAO seems to not have been deployed yet".to_string(),
            ))
        }

        // Check that we have the exec key
        if dao.params.exec_secret_key.is_none() {
            return Err(Error::Custom(
                "[dao_exec_transfer] We need the exec secret key to execute proposals for this DAO"
                    .to_string(),
            ))
        }

        // If early flag is provided, check that we have the early exec key
        if early && dao.params.early_exec_secret_key.is_none() {
            return Err(Error::Custom(
                "[dao_exec_transfer] We need the early exec secret key to execute proposals early for this DAO"
                    .to_string(),
            ))
        }

        // Check proposal is approved
        let votes = self.get_dao_proposal_votes(&proposal.bulla()).await?;
        let mut yes_vote_value = 0;
        let mut yes_vote_blind = Blind::ZERO;
        let mut all_vote_value = 0;
        let mut all_vote_blind = Blind::ZERO;
        for vote in votes {
            if vote.vote_option {
                yes_vote_value += vote.all_vote_value;
            };
            yes_vote_blind += vote.yes_vote_blind;
            all_vote_value += vote.all_vote_value;
            all_vote_blind += vote.all_vote_blind;
        }
        let approval_ratio = (yes_vote_value as f64 * 100.0) / all_vote_value as f64;
        if all_vote_value < dao.params.dao.quorum ||
            approval_ratio <
                (dao.params.dao.approval_ratio_quot / dao.params.dao.approval_ratio_base)
                    as f64
        {
            return Err(Error::Custom(
                "[dao_exec_transfer] Proposal is not approved yet".to_string(),
            ))
        };

        // Fetch DAO unspent OwnCoins to see what its balance is for the coin
        let dao_spend_hook =
            FuncRef { contract_id: *DAO_CONTRACT_ID, func_code: DaoFunction::Exec as u8 }
                .to_func_id();
        let dao_owncoins = self
            .get_contract_token_coins(
                &proposal_coinattrs.token_id,
                &dao_spend_hook,
                &proposal.proposal.dao_bulla.inner(),
            )
            .await?;
        if dao_owncoins.is_empty() {
            return Err(Error::Custom(format!(
                "[dao_exec_transfer] Did not find any {} unspent coins owned by this DAO",
                proposal_coinattrs.token_id,
            )))
        }

        // Check DAO balance is sufficient
        if dao_owncoins.iter().map(|x| x.note.value).sum::<u64>() < proposal_coinattrs.value {
            return Err(Error::Custom(format!(
                "[dao_exec_transfer] Not enough DAO balance for token ID: {}",
                proposal_coinattrs.token_id,
            )))
        }

        // Find which DAO coins we can use
        let (spent_coins, change_value) = select_coins(dao_owncoins, proposal_coinattrs.value)?;

        // Now we need to do a lookup for the zkas proof bincodes, and create
        // the circuit objects and proving keys so we can build the transaction.
        // We also do this through the RPC. First we grab the calls from money.
        let zkas_bins = self.lookup_zkas(&MONEY_CONTRACT_ID).await?;

        let Some(mint_zkbin) = zkas_bins.iter().find(|x| x.0 == MONEY_CONTRACT_ZKAS_MINT_NS_V1)
        else {
            return Err(Error::Custom("[dao_exec_transfer] Mint circuit not found".to_string()))
        };

        let Some(burn_zkbin) = zkas_bins.iter().find(|x| x.0 == MONEY_CONTRACT_ZKAS_BURN_NS_V1)
        else {
            return Err(Error::Custom("[dao_exec_transfer] Burn circuit not found".to_string()))
        };

        let Some(fee_zkbin) = zkas_bins.iter().find(|x| x.0 == MONEY_CONTRACT_ZKAS_FEE_NS_V1)
        else {
            return Err(Error::Custom("[dao_exec_transfer] Fee circuit not found".to_string()))
        };

        let mint_zkbin = ZkBinary::decode(&mint_zkbin.1)?;
        let burn_zkbin = ZkBinary::decode(&burn_zkbin.1)?;
        let fee_zkbin = ZkBinary::decode(&fee_zkbin.1)?;

        let mint_circuit = ZkCircuit::new(empty_witnesses(&mint_zkbin)?, &mint_zkbin);
        let burn_circuit = ZkCircuit::new(empty_witnesses(&burn_zkbin)?, &burn_zkbin);
        let fee_circuit = ZkCircuit::new(empty_witnesses(&fee_zkbin)?, &fee_zkbin);

        // Creating Mint, Burn and Fee circuits proving keys
        let mint_pk = ProvingKey::build(mint_zkbin.k, &mint_circuit);
        let burn_pk = ProvingKey::build(burn_zkbin.k, &burn_circuit);
        let fee_pk = ProvingKey::build(fee_zkbin.k, &fee_circuit);

        // Now we grab the DAO bins
        let zkas_bins = self.lookup_zkas(&DAO_CONTRACT_ID).await?;

        let (namespace, early_exec_secret_key) = match early {
            true => (
                DAO_CONTRACT_ZKAS_DAO_EARLY_EXEC_NS,
                Some(dao.params.early_exec_secret_key.unwrap()),
            ),
            false => (DAO_CONTRACT_ZKAS_DAO_EXEC_NS, None),
        };

        let Some(dao_exec_zkbin) = zkas_bins.iter().find(|x| x.0 == namespace) else {
            return Err(Error::Custom(format!(
                "[dao_exec_transfer] DAO {namespace} circuit not found"
            )))
        };

        let Some(dao_auth_transfer_zkbin) =
            zkas_bins.iter().find(|x| x.0 == DAO_CONTRACT_ZKAS_DAO_AUTH_MONEY_TRANSFER_NS)
        else {
            return Err(Error::Custom(
                "[dao_exec_transfer] DAO AuthTransfer circuit not found".to_string(),
            ))
        };

        let Some(dao_auth_transfer_enc_coin_zkbin) =
            zkas_bins.iter().find(|x| x.0 == DAO_CONTRACT_ZKAS_DAO_AUTH_MONEY_TRANSFER_ENC_COIN_NS)
        else {
            return Err(Error::Custom(
                "[dao_exec_transfer] DAO AuthTransferEncCoin circuit not found".to_string(),
            ))
        };

        let dao_exec_zkbin = ZkBinary::decode(&dao_exec_zkbin.1)?;
        let dao_auth_transfer_zkbin = ZkBinary::decode(&dao_auth_transfer_zkbin.1)?;
        let dao_auth_transfer_enc_coin_zkbin =
            ZkBinary::decode(&dao_auth_transfer_enc_coin_zkbin.1)?;

        let dao_exec_circuit = ZkCircuit::new(empty_witnesses(&dao_exec_zkbin)?, &dao_exec_zkbin);
        let dao_auth_transfer_circuit =
            ZkCircuit::new(empty_witnesses(&dao_auth_transfer_zkbin)?, &dao_auth_transfer_zkbin);
        let dao_auth_transfer_enc_coin_circuit = ZkCircuit::new(
            empty_witnesses(&dao_auth_transfer_enc_coin_zkbin)?,
            &dao_auth_transfer_enc_coin_zkbin,
        );

        // Creating DAO Exec, AuthTransfer and AuthTransferEncCoin circuits proving keys
        let dao_exec_pk = ProvingKey::build(dao_exec_zkbin.k, &dao_exec_circuit);
        let dao_auth_transfer_pk =
            ProvingKey::build(dao_auth_transfer_zkbin.k, &dao_auth_transfer_circuit);
        let dao_auth_transfer_enc_coin_pk = ProvingKey::build(
            dao_auth_transfer_enc_coin_zkbin.k,
            &dao_auth_transfer_enc_coin_circuit,
        );

        // Fetch our money Merkle tree
        let tree = self.get_money_tree().await?;

        // Retrieve next block height and current block time target,
        // to compute their window.
        let next_block_height = self.get_next_block_height().await?;
        let block_target = self.get_block_target().await?;
        let current_blockwindow = blockwindow(next_block_height, block_target);

        // Now we can create the transfer call parameters
        let input_user_data_blind = Blind::random(&mut OsRng);
        let mut inputs = vec![];
        for coin in &spent_coins {
            inputs.push(TransferCallInput {
                coin: coin.clone(),
                merkle_path: tree.witness(coin.leaf_position, 0).unwrap(),
                user_data_blind: input_user_data_blind,
            });
        }

        let mut outputs = vec![];
        outputs.push(proposal_coinattrs.clone());

        let dao_coin_attrs = CoinAttributes {
            public_key: dao.params.dao.notes_public_key,
            value: change_value,
            token_id: proposal_coinattrs.token_id,
            spend_hook: dao_spend_hook,
            user_data: proposal.proposal.dao_bulla.inner(),
            blind: Blind::random(&mut OsRng),
        };
        outputs.push(dao_coin_attrs.clone());

        // Create the transfer call
        let transfer_builder = TransferCallBuilder {
            clear_inputs: vec![],
            inputs,
            outputs,
            mint_zkbin: mint_zkbin.clone(),
            mint_pk: mint_pk.clone(),
            burn_zkbin: burn_zkbin.clone(),
            burn_pk: burn_pk.clone(),
        };
        let (transfer_params, transfer_secrets) = transfer_builder.build()?;

        // Encode the call
        let mut data = vec![MoneyFunction::TransferV1 as u8];
        transfer_params.encode_async(&mut data).await?;
        let transfer_call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };

        // Create the exec call
        let exec_signature_secret = SecretKey::random(&mut OsRng);
        let exec_builder = DaoExecCall {
            proposal: proposal.proposal.clone(),
            dao: dao.params.dao.clone(),
            yes_vote_value,
            all_vote_value,
            yes_vote_blind,
            all_vote_blind,
            signature_secret: exec_signature_secret,
            current_blockwindow,
        };
        let (exec_params, exec_proofs) = exec_builder.make(
            &dao.params.exec_secret_key.unwrap(),
            &early_exec_secret_key,
            &dao_exec_zkbin,
            &dao_exec_pk,
        )?;

        // Encode the call
        let mut data = vec![DaoFunction::Exec as u8];
        exec_params.encode_async(&mut data).await?;
        let exec_call = ContractCall { contract_id: *DAO_CONTRACT_ID, data };

        // Now we can create the auth call
        // Auth module
        let auth_transfer_builder = DaoAuthMoneyTransferCall {
            proposal: proposal.proposal.clone(),
            proposal_coinattrs: vec![proposal_coinattrs],
            dao: dao.params.dao.clone(),
            input_user_data_blind,
            dao_coin_attrs,
        };
        let (auth_transfer_params, auth_transfer_proofs) = auth_transfer_builder.make(
            &dao_auth_transfer_zkbin,
            &dao_auth_transfer_pk,
            &dao_auth_transfer_enc_coin_zkbin,
            &dao_auth_transfer_enc_coin_pk,
        )?;

        // Encode the call
        let mut data = vec![DaoFunction::AuthMoneyTransfer as u8];
        auth_transfer_params.encode_async(&mut data).await?;
        let auth_transfer_call = ContractCall { contract_id: *DAO_CONTRACT_ID, data };

        // Create the TransactionBuilder containing above calls
        let mut tx_builder = TransactionBuilder::new(
            ContractCallLeaf { call: exec_call, proofs: exec_proofs },
            vec![
                DarkTree::new(
                    ContractCallLeaf { call: auth_transfer_call, proofs: auth_transfer_proofs },
                    vec![],
                    None,
                    None,
                ),
                DarkTree::new(
                    ContractCallLeaf { call: transfer_call, proofs: transfer_secrets.proofs },
                    vec![],
                    None,
                    None,
                ),
            ],
        )?;

        // We first have to execute the fee-less tx to gather its used gas, and then we feed
        // it into the fee-creating function.
        let mut tx = tx_builder.build()?;
        let auth_transfer_sigs = tx.create_sigs(&[])?;
        let transfer_sigs = tx.create_sigs(&transfer_secrets.signature_secrets)?;
        let exec_sigs = tx.create_sigs(&[exec_signature_secret])?;
        tx.signatures = vec![auth_transfer_sigs, transfer_sigs, exec_sigs];

        let (fee_call, fee_proofs, fee_secrets) =
            self.append_fee_call(&tx, &tree, &fee_pk, &fee_zkbin, None).await?;

        // Append the fee call to the transaction
        tx_builder.append(ContractCallLeaf { call: fee_call, proofs: fee_proofs }, vec![])?;

        // Now build the actual transaction and sign it with all necessary keys.
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&[])?;
        tx.signatures.push(sigs);
        let sigs = tx.create_sigs(&transfer_secrets.signature_secrets)?;
        tx.signatures.push(sigs);
        let sigs = tx.create_sigs(&[exec_signature_secret])?;
        tx.signatures.push(sigs);
        let sigs = tx.create_sigs(&fee_secrets)?;
        tx.signatures.push(sigs);

        Ok(tx)
    }

    /// Execute a DAO generic proposal.
    pub async fn dao_exec_generic(
        &self,
        proposal: &ProposalRecord,
        early: bool,
    ) -> Result<Transaction> {
        if proposal.leaf_position.is_none() ||
            proposal.money_snapshot_tree.is_none() ||
            proposal.nullifiers_smt_snapshot.is_none() ||
            proposal.tx_hash.is_none() ||
            proposal.call_index.is_none()
        {
            return Err(Error::Custom(
                "[dao_exec_generic] Proposal seems to not have been deployed yet".to_string(),
            ))
        }

        // Check proposal is not executed
        if let Some(exec_tx_hash) = proposal.exec_tx_hash {
            return Err(Error::Custom(format!(
                "[dao_exec_generic] Proposal was executed on transaction: {exec_tx_hash}"
            )))
        }

        // Fetch DAO and check its deployed
        let Ok(dao) = self.get_dao_by_bulla(&proposal.proposal.dao_bulla).await else {
            return Err(Error::Custom(format!(
                "[dao_exec_generic] DAO {} was not found",
                proposal.proposal.dao_bulla
            )))
        };
        if dao.leaf_position.is_none() || dao.tx_hash.is_none() || dao.call_index.is_none() {
            return Err(Error::Custom(
                "[dao_exec_generic] DAO seems to not have been deployed yet".to_string(),
            ))
        }

        // Check that we have the exec key
        if dao.params.exec_secret_key.is_none() {
            return Err(Error::Custom(
                "[dao_exec_generic] We need the exec secret key to execute proposals for this DAO"
                    .to_string(),
            ))
        }

        // If early flag is provided, check that we have the early exec key
        if early && dao.params.early_exec_secret_key.is_none() {
            return Err(Error::Custom(
                "[dao_exec_generic] We need the early exec secret key to execute proposals early for this DAO"
                    .to_string(),
            ))
        }

        // Check proposal is approved
        let votes = self.get_dao_proposal_votes(&proposal.bulla()).await?;
        let mut yes_vote_value = 0;
        let mut yes_vote_blind = Blind::ZERO;
        let mut all_vote_value = 0;
        let mut all_vote_blind = Blind::ZERO;
        for vote in votes {
            if vote.vote_option {
                yes_vote_value += vote.all_vote_value;
            };
            yes_vote_blind += vote.yes_vote_blind;
            all_vote_value += vote.all_vote_value;
            all_vote_blind += vote.all_vote_blind;
        }
        let approval_ratio = (yes_vote_value as f64 * 100.0) / all_vote_value as f64;
        if all_vote_value < dao.params.dao.quorum ||
            approval_ratio <
                (dao.params.dao.approval_ratio_quot / dao.params.dao.approval_ratio_base)
                    as f64
        {
            return Err(Error::Custom("[dao_exec_generic] Proposal is not approved yet".to_string()))
        };

        // Now we need to do a lookup for the zkas proof bincodes, and create
        // the circuit objects and proving keys so we can build the transaction.
        // We also do this through the RPC. First we grab the calls from money.
        let zkas_bins = self.lookup_zkas(&MONEY_CONTRACT_ID).await?;
        let Some(fee_zkbin) = zkas_bins.iter().find(|x| x.0 == MONEY_CONTRACT_ZKAS_FEE_NS_V1)
        else {
            return Err(Error::Custom("[dao_exec_generic] Fee circuit not found".to_string()))
        };
        let fee_zkbin = ZkBinary::decode(&fee_zkbin.1)?;
        let fee_circuit = ZkCircuit::new(empty_witnesses(&fee_zkbin)?, &fee_zkbin);
        let fee_pk = ProvingKey::build(fee_zkbin.k, &fee_circuit);

        // Now we grab the DAO bins
        let zkas_bins = self.lookup_zkas(&DAO_CONTRACT_ID).await?;

        let (namespace, early_exec_secret_key) = match early {
            true => (
                DAO_CONTRACT_ZKAS_DAO_EARLY_EXEC_NS,
                Some(dao.params.early_exec_secret_key.unwrap()),
            ),
            false => (DAO_CONTRACT_ZKAS_DAO_EXEC_NS, None),
        };

        let Some(dao_exec_zkbin) = zkas_bins.iter().find(|x| x.0 == namespace) else {
            return Err(Error::Custom(format!(
                "[dao_exec_generic] DAO {namespace} circuit not found"
            )))
        };
        let dao_exec_zkbin = ZkBinary::decode(&dao_exec_zkbin.1)?;
        let dao_exec_circuit = ZkCircuit::new(empty_witnesses(&dao_exec_zkbin)?, &dao_exec_zkbin);
        let dao_exec_pk = ProvingKey::build(dao_exec_zkbin.k, &dao_exec_circuit);

        // Fetch our money Merkle tree
        let tree = self.get_money_tree().await?;

        // Retrieve next block height and current block time target,
        // to compute their window.
        let next_block_height = self.get_next_block_height().await?;
        let block_target = self.get_block_target().await?;
        let current_blockwindow = blockwindow(next_block_height, block_target);

        // Create the exec call
        let exec_signature_secret = SecretKey::random(&mut OsRng);
        let exec_builder = DaoExecCall {
            proposal: proposal.proposal.clone(),
            dao: dao.params.dao.clone(),
            yes_vote_value,
            all_vote_value,
            yes_vote_blind,
            all_vote_blind,
            signature_secret: exec_signature_secret,
            current_blockwindow,
        };
        let (exec_params, exec_proofs) = exec_builder.make(
            &dao.params.exec_secret_key.unwrap(),
            &early_exec_secret_key,
            &dao_exec_zkbin,
            &dao_exec_pk,
        )?;

        // Encode the call
        let mut data = vec![DaoFunction::Exec as u8];
        exec_params.encode_async(&mut data).await?;
        let exec_call = ContractCall { contract_id: *DAO_CONTRACT_ID, data };

        // Create the TransactionBuilder containing above calls
        let mut tx_builder = TransactionBuilder::new(
            ContractCallLeaf { call: exec_call, proofs: exec_proofs },
            vec![],
        )?;

        // We first have to execute the fee-less tx to gather its used gas, and then we feed
        // it into the fee-creating function.
        let mut tx = tx_builder.build()?;
        let exec_sigs = tx.create_sigs(&[exec_signature_secret])?;
        tx.signatures = vec![exec_sigs];

        let (fee_call, fee_proofs, fee_secrets) =
            self.append_fee_call(&tx, &tree, &fee_pk, &fee_zkbin, None).await?;

        // Append the fee call to the transaction
        tx_builder.append(ContractCallLeaf { call: fee_call, proofs: fee_proofs }, vec![])?;

        // Now build the actual transaction and sign it with all necessary keys.
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&[exec_signature_secret])?;
        tx.signatures.push(sigs);
        let sigs = tx.create_sigs(&fee_secrets)?;
        tx.signatures.push(sigs);

        Ok(tx)
    }
}
