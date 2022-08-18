use incrementalmerkletree::{bridgetree::BridgeTree, Tree};
use pasta_curves::pallas;
use std::any::{Any, TypeId};

use darkfi::{
    crypto::{
        constants::MERKLE_DEPTH,
        keypair::{Keypair, PublicKey, SecretKey},
        merkle_node::MerkleNode,
        nullifier::Nullifier,
        proof::VerifyingKey,
    },
    node::state::{ProgramState, StateUpdate},
};

use crate::{
    dao_contract::mint::validate::CallData,
    demo::{StateRegistry, Transaction},
    Result,
};

#[derive(Clone)]
pub struct DaoBulla(pub pallas::Base);

type MerkleTree = BridgeTree<MerkleNode, MERKLE_DEPTH>;

/// This DAO state is for all DAOs on the network. There should only be a single instance.
pub struct State {
    dao_bullas: Vec<DaoBulla>,
    pub dao_tree: MerkleTree,
    pub dao_roots: Vec<MerkleNode>,

    proposal_bullas: Vec<pallas::Base>,
    pub proposal_tree: MerkleTree,
    pub proposal_roots: Vec<MerkleNode>,
}

impl State {
    pub fn new() -> Box<dyn Any> {
        Box::new(Self {
            dao_bullas: Vec::new(),
            dao_tree: MerkleTree::new(100),
            dao_roots: Vec::new(),
            proposal_bullas: Vec::new(),
            proposal_tree: MerkleTree::new(100),
            proposal_roots: Vec::new(),
        })
    }

    pub fn add_dao_bulla(&mut self, bulla: DaoBulla) {
        let node = MerkleNode(bulla.0);
        self.dao_bullas.push(bulla);
        self.dao_tree.append(&node);
        self.dao_roots.push(self.dao_tree.root(0).unwrap());
    }

    pub fn add_proposal_bulla(&mut self, bulla: pallas::Base) {
        let node = MerkleNode(bulla);
        self.proposal_bullas.push(bulla);
        self.proposal_tree.append(&node);
        self.proposal_roots.push(self.proposal_tree.root(0).unwrap());
    }

    pub fn is_valid_dao_merkle(&self, root: &MerkleNode) -> bool {
        self.dao_roots.iter().any(|m| m == root)
    }

    pub fn is_valid_proposal_merkle(&self, root: &MerkleNode) -> bool {
        self.proposal_roots.iter().any(|m| m == root)
    }
}
