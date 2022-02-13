use chrono::Utc;
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use super::{block::Block, blockchain::Blockchain, vote::Vote};

/// This struct represents a protocol node.
/// Each node is numbered and has a secret-public keys pair, to sign messages.
/// Nodes hold a set of Blockchains(some of which are not notarized)
/// and a set of unconfirmed pending transactions.
#[derive(Debug)]
pub struct Node {
    pub id: u64,
    pub genesis_time: i64,
    pub password: String,
    pub private_key: String,
    pub public_key: String,
    pub canonical_blockchain: Blockchain,
    pub node_blockchains: Vec<Blockchain>,
    pub unconfirmed_transactions: Vec<String>,
}

impl Node {
    pub fn new(id: u64, genesis_time: i64, password: String, init_block: Block) -> Node {
        // TODO: add keypair generation, clock sync
        Node {
            id,
            genesis_time,
            password,
            private_key: String::from("private_key"),
            public_key: String::from("public_key"),
            canonical_blockchain: Blockchain::new(init_block),
            node_blockchains: Vec::new(),
            unconfirmed_transactions: Vec::new(),
        }
    }

    /// A nodes output is the finalized (canonical) blockchain they hold.
    pub fn output(&self) -> &Blockchain {
        &self.canonical_blockchain
    }

    /// Node retreives a transaction and append it to the unconfirmed transactions list.
    /// Additional validity rules must be defined by the protocol for its blockchain data structure.
    pub fn receive_transaction(&mut self, transaction: String) {
        self.unconfirmed_transactions.push(transaction);
    }

    /// Node broadcast a transaction to provided nodes list.
    pub fn broadcast_transaction(&mut self, nodes: Vec<&mut Node>, transaction: String) {
        for node in nodes {
            node.receive_transaction(transaction.clone())
        }
    }

    /// Node calculates current epoch, based on elapsed time from the genesis block.
    /// Epochs duration is configured using the delta value.
    pub fn get_current_epoch(&self) -> i64 {
        let delta = 2;
        let current_time = Utc::now().timestamp();
        ((current_time - self.genesis_time) % (2 * delta)) + 1
    }

    /// Node finds epochs leader, using a simple hash method.
    /// Leader calculation is based on how many nodes are participating in the network.
    pub fn get_epoch_leader(&self, nodes_count: u64) -> u64 {
        let epoch = self.get_current_epoch();
        let mut hasher = DefaultHasher::new();
        epoch.hash(&mut hasher);
        hasher.finish() % nodes_count
    }

    /// Node checks if they are the current epoch leader.
    pub fn check_if_epoch_leader(&self, nodes_count: u64) -> bool {
        let leader = self.get_epoch_leader(nodes_count);
        self.id == leader
    }

    /// Node generates a block for the current, containing all uncorfirmed transactions.
    /// Block extends the longest notarized blockchain the node holds.
    pub fn propose_block(&self) -> Block {
        let epoch = self.get_current_epoch();
        let longest_notarized_chain = self.find_longest_notarized_chain();
        let mut hasher = DefaultHasher::new();
        longest_notarized_chain.blocks.last().unwrap().hash(&mut hasher);
        let proposed_block =
            Block::new(hasher.finish().to_string(), epoch, self.unconfirmed_transactions.clone());
        // TODO: add block signing.
        proposed_block
    }

    /// Node receives the proposed block, verifies its sender(epoch leader), and proceeds with voting on it.
    pub fn receive_proposed_block(&mut self, proposed_block: &Block) -> Option<Vote> {
        // TODO: verify leader keys.
        self.vote_block(proposed_block)
    }

    /// Given a block, node finds which blockchain it extends.
    /// If block extends the canonical blockchain, a new fork blockchain is created.
    /// Node votes on the block, only if it extends the longest notarized chain it has seen.
    pub fn vote_block(&mut self, block: &Block) -> Option<Vote> {
        let index = self.find_extended_blockchain_index(block);

        let blockchain = if index == -1 {
            let blockchain = Blockchain::new(block.clone());
            self.node_blockchains.push(blockchain);
            self.node_blockchains.last().unwrap()
        } else {
            self.node_blockchains[index as usize].add_block(&block);
            &self.node_blockchains[index as usize]
        };

        if self.extends_notarized_blockchain(blockchain) {
            // TODO: add block signing.
            return Some(Vote::new(String::from("signed_block"), block.clone(), self.id))
        }
        None
    }

    /// Node verifies if provided blockchain is notarized excluding the last block.
    pub fn extends_notarized_blockchain(&self, blockchain: &Blockchain) -> bool {
        for block in &blockchain.blocks[..(blockchain.blocks.len() - 1)] {
            if !block.notarized {
                return false
            }
        }
        true
    }

    /// Given a block, node finds the index of the blockchain it extends.
    pub fn find_extended_blockchain_index(&self, block: &Block) -> i64 {
        let mut hasher = DefaultHasher::new();
        for (index, blockchain) in self.node_blockchains.iter().enumerate() {
            blockchain.blocks.last().unwrap().hash(&mut hasher);
            if block.h == hasher.finish().to_string() &&
                block.e > blockchain.blocks.last().unwrap().e
            {
                return index as i64
            }
        }

        self.canonical_blockchain.blocks.last().unwrap().hash(&mut hasher);
        if block.h != hasher.finish().to_string() ||
            block.e <= self.canonical_blockchain.blocks.last().unwrap().e
        {
            panic!("Proposed block doesn't extend any known chains.");
        }
        -1
    }

    /// Finds the longest fully notarized blockchain the node holds.
    pub fn find_longest_notarized_chain(&self) -> &Blockchain {
        let mut longest_notarized_chain = &self.canonical_blockchain;
        let mut length = 0;
        for blockchain in &self.node_blockchains {
            if blockchain.is_notarized() && blockchain.blocks.len() > length {
                length = blockchain.blocks.len();
                longest_notarized_chain = &blockchain;
            }
        }
        &longest_notarized_chain
    }

    /// Node receives a vote for a block.
    /// First, sender is verified using their public key.
    /// Block is searched in nodes blockchains.
    /// If the vote wasn't received before, it is appended to block votes list.
    /// When a node sees 2n/3 votes for a block it notarizes it.
    /// When a block gets notarized, the transactions it contains are removed from
    /// nodes unconfirmed transactions list.
    /// Finally, we check if the notarization of the block can finalize parent blocks
    ///	in its blockchain.
    pub fn receive_vote(&mut self, vote: &Vote, nodes_count: usize) -> Option<Vote> {
        // TODO: verify vote signature.
        let vote_block = self.find_block(&vote.block);
        if vote_block == None {
            return self.vote_block(&vote.block)
        }

        let (unwrapped_vote_block, blockchain_index) = vote_block.unwrap();
        if !unwrapped_vote_block.votes.contains(vote) {
            unwrapped_vote_block.votes.push(vote.clone());
        }

        if !unwrapped_vote_block.notarized &&
            unwrapped_vote_block.votes.len() > (2 * nodes_count / 3)
        {
            unwrapped_vote_block.notarized = true;

            for transaction in unwrapped_vote_block.txs.clone() {
                let txs_clone = transaction.clone();
                if let Some(pos) =
                    self.unconfirmed_transactions.iter().position(|txs| *txs == txs_clone)
                {
                    self.unconfirmed_transactions.remove(pos);
                }
            }

            self.check_blockchain_finalization(blockchain_index);
        }
        None
    }

    /// Node searches it the blockchains it holds for provided block.
    pub fn find_block(&mut self, vote_block: &Block) -> Option<(&mut Block, i64)> {
        for (index, blockchain) in &mut self.node_blockchains.iter_mut().enumerate() {
            for block in blockchain.blocks.iter_mut().rev() {
                if vote_block == block {
                    return Some((block, index as i64))
                }
            }
        }

        for block in &mut self.canonical_blockchain.blocks.iter_mut().rev() {
            if vote_block == block {
                return Some((block, -1))
            }
        }
        None
    }

    /// Node checks if the index blockchain can be finalized.
    /// Consensus finalization logic: If node has observed the notarization of 3 consecutive
    /// blocks in a fork chain, it finalizes (appends to canonical blockchain) all blocks up to the middle block.
    /// When fork chain blocks are finalized, rest fork chains not starting by those blocks are removed.
    pub fn check_blockchain_finalization(&mut self, blockchain_index: i64) {
        let blockchain = if blockchain_index == -1 {
            &mut self.canonical_blockchain
        } else {
            &mut self.node_blockchains[blockchain_index as usize]
        };

        let blockchain_len = blockchain.blocks.len();
        if blockchain_len > 2 {
            let mut consecutive_notarized = 0;
            for block in &blockchain.blocks {
                if block.notarized {
                    consecutive_notarized = consecutive_notarized + 1;
                } else {
                    break
                }
            }

            if consecutive_notarized > 2 {
                let mut finalized_blocks = Vec::new();
                for block in &mut blockchain.blocks[..(consecutive_notarized - 1)] {
                    block.finalized = true;
                    finalized_blocks.push(block.clone());
                }
                blockchain.blocks.drain(0..(consecutive_notarized - 1));
                for block in &finalized_blocks {
                    self.canonical_blockchain.blocks.push(block.clone());
                }

                let mut hasher = DefaultHasher::new();
                let last_finalized_block = self.canonical_blockchain.blocks.last().unwrap();
                last_finalized_block.hash(&mut hasher);
                let last_finalized_block_hash = hasher.finish().to_string();
                let mut dropped_blockchains = Vec::new();
                for (index, blockchain) in self.node_blockchains.iter().enumerate() {
                    let first_block = blockchain.blocks.first().unwrap();
                    if first_block.h != last_finalized_block_hash ||
                        first_block.e <= last_finalized_block.e
                    {
                        dropped_blockchains.push(index);
                    }
                }
                for index in dropped_blockchains {
                    self.node_blockchains.remove(index);
                }
            }
        }
    }
}
