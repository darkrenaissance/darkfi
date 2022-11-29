/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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

use darkfi_sdk::crypto::{constants::MERKLE_DEPTH, MerkleNode};
use darkfi_serial::{SerialDecodable, SerialEncodable};
use incrementalmerkletree::bridgetree::BridgeTree;
use pasta_curves::pallas;

use super::{constants, leadcoin::LeadCoin, Block, Float10, ProposalChain};

use crate::{net, tx::Transaction, util::time::Timestamp, Result};

/// This struct represents the information required by the consensus algorithm
#[derive(Debug)]
pub struct ConsensusState {
    /// Genesis block creation timestamp
    pub genesis_ts: Timestamp,
    /// Genesis block hash
    pub genesis_block: blake3::Hash,
    /// Participating start slot
    pub participating: Option<u64>,
    /// Last slot node check for finalization
    pub checked_finalization: u64,
    /// Slots offset since genesis,
    pub offset: Option<u64>,
    /// Fork chains containing block proposals
    pub proposals: Vec<ProposalChain>,
    /// Current epoch
    pub epoch: u64,
    /// Hot/live slot checkpoints
    pub slot_checkpoints: Vec<SlotCheckpoint>,
    /// previous epoch eta
    pub prev_epoch_eta: pallas::Base,
    /// Current epoch eta
    pub epoch_eta: pallas::Base,
    // TODO: Aren't these already in db after finalization?
    /// Current epoch competing coins
    pub coins: Vec<Vec<LeadCoin>>,
    /// Coin commitments tree
    pub coins_tree: BridgeTree<MerkleNode, MERKLE_DEPTH>,
    /// Seen nullifiers from proposals
    pub leaders_nullifiers: Vec<pallas::Base>,
    /// Seen spent coins from proposals
    pub leaders_spent_coins: Vec<(pallas::Base, pallas::Base)>,
    /// Leaders count history
    pub leaders_history: Vec<u64>,
    /// Kp
    pub kp: Float10,
    /// Previous slot sigma1
    pub prev_sigma1: pallas::Base,
    /// Previous slot sigma2
    pub prev_sigma2: pallas::Base,
}

impl ConsensusState {
    pub fn new(genesis_ts: Timestamp, genesis_data: blake3::Hash) -> Result<Self> {
        let genesis_block = Block::genesis_block(genesis_ts, genesis_data).blockhash();
        Ok(Self {
            genesis_ts,
            genesis_block,
            participating: None,
            checked_finalization: 0,
            offset: None,
            proposals: vec![],
            epoch: 0,
            slot_checkpoints: vec![],
            prev_epoch_eta: pallas::Base::one(),
            epoch_eta: pallas::Base::one(),
            coins: vec![],
            coins_tree: BridgeTree::<MerkleNode, MERKLE_DEPTH>::new(constants::EPOCH_LENGTH * 100),
            leaders_nullifiers: vec![],
            leaders_spent_coins: vec![],
            leaders_history: vec![0],
            kp: constants::FLOAT10_TWO.clone() / constants::FLOAT10_NINE.clone(),
            prev_sigma1: pallas::Base::zero(),
            prev_sigma2: pallas::Base::zero(),
        })
    }
}

/// Auxiliary structure used for consensus syncing.
#[derive(Debug, SerialEncodable, SerialDecodable)]
pub struct ConsensusRequest {}

impl net::Message for ConsensusRequest {
    fn name() -> &'static str {
        "consensusrequest"
    }
}

/// Auxiliary structure used for consensus syncing.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ConsensusResponse {
    /// Slots offset since genesis,
    pub offset: Option<u64>,
    /// Hot/live data used by the consensus algorithm
    pub proposals: Vec<ProposalChain>,
    /// Pending transactions
    pub unconfirmed_txs: Vec<Transaction>,
    /// Hot/live slot checkpoints
    pub slot_checkpoints: Vec<SlotCheckpoint>,
    /// Seen nullifiers from proposals
    pub leaders_nullifiers: Vec<pallas::Base>,
    /// Seen spent coins from proposals
    pub leaders_spent_coins: Vec<(pallas::Base, pallas::Base)>,
}

impl net::Message for ConsensusResponse {
    fn name() -> &'static str {
        "consensusresponse"
    }
}

/// Auxiliary structure used to keep track of slot validation parameters.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SlotCheckpoint {
    /// Slot UID
    pub slot: u64,
    /// Slot eta
    pub eta: pallas::Base,
    /// Slot sigma1
    pub sigma1: pallas::Base,
    /// Slot sigma2
    pub sigma2: pallas::Base,
}

impl SlotCheckpoint {
    pub fn new(slot: u64, eta: pallas::Base, sigma1: pallas::Base, sigma2: pallas::Base) -> Self {
        Self { slot, eta, sigma1, sigma2 }
    }

    /// Generate the genesis slot checkpoint.
    pub fn genesis_slot_checkpoint() -> Self {
        let eta = pallas::Base::zero();
        let sigma1 = pallas::Base::zero();
        let sigma2 = pallas::Base::zero();

        Self::new(0, eta, sigma1, sigma2)
    }
}

/// Auxiliary structure used for slot checkpoints syncing
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SlotCheckpointRequest {
    /// Slot UID
    pub slot: u64,
}

impl net::Message for SlotCheckpointRequest {
    fn name() -> &'static str {
        "slotcheckpointrequest"
    }
}

/// Auxiliary structure used for slot checkpoints syncing
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SlotCheckpointResponse {
    /// Response blocks.
    pub slot_checkpoints: Vec<SlotCheckpoint>,
}

impl net::Message for SlotCheckpointResponse {
    fn name() -> &'static str {
        "slotcheckpointresponse"
    }
}
