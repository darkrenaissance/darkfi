use incrementalmerkletree::{bridgetree::BridgeTree, Tree};
use pasta_curves::{
    arithmetic::CurveAffine,
    group::{
        ff::{Field, PrimeField},
        Curve, Group, GroupEncoding,
    },
    pallas,
};
use std::{
    any::{Any, TypeId},
    collections::HashMap,
    hash::{Hash, Hasher},
};

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

#[derive(Eq, PartialEq)]
pub struct HashableBase(pallas::Base);

impl std::hash::Hash for HashableBase {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let bytes = self.0.to_repr();
        bytes.hash(state);
    }
}

pub struct ProposalVotes {
    // TODO: might be more logical to have 'yes_vote_commits' and 'no_vote_commits'
    /// Weighted vote commits
    pub vote_commits: pallas::Point,
    /// All value staked in the vote
    pub value_commits: pallas::Point,
    /// Vote nullifiers
    pub vote_nulls: Vec<Nullifier>,
}

impl ProposalVotes {
    pub fn nullifier_exists(&self, nullifier: &Nullifier) -> bool {
        self.vote_nulls.iter().any(|n| n == nullifier)
    }
}

/// This DAO state is for all DAOs on the network. There should only be a single instance.
pub struct State {
    dao_bullas: Vec<DaoBulla>,
    pub dao_tree: MerkleTree,
    pub dao_roots: Vec<MerkleNode>,

    //proposal_bullas: Vec<pallas::Base>,
    pub proposal_tree: MerkleTree,
    pub proposal_roots: Vec<MerkleNode>,
    proposal_votes: HashMap<HashableBase, ProposalVotes>,
}

impl State {
    pub fn new() -> Box<dyn Any> {
        Box::new(Self {
            dao_bullas: Vec::new(),
            dao_tree: MerkleTree::new(100),
            dao_roots: Vec::new(),
            //proposal_bullas: Vec::new(),
            proposal_tree: MerkleTree::new(100),
            proposal_roots: Vec::new(),
            proposal_votes: HashMap::new(),
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
        //self.proposal_bullas.push(bulla);
        self.proposal_tree.append(&node);
        self.proposal_roots.push(self.proposal_tree.root(0).unwrap());
        self.proposal_votes.insert(
            HashableBase(bulla),
            ProposalVotes {
                vote_commits: pallas::Point::identity(),
                value_commits: pallas::Point::identity(),
                vote_nulls: Vec::new(),
            },
        );
    }

    pub fn lookup_proposal_votes(&self, proposal_bulla: pallas::Base) -> Option<&ProposalVotes> {
        self.proposal_votes.get(&HashableBase(proposal_bulla))
    }
    pub fn lookup_proposal_votes_mut(
        &mut self,
        proposal_bulla: pallas::Base,
    ) -> Option<&mut ProposalVotes> {
        self.proposal_votes.get_mut(&HashableBase(proposal_bulla))
    }

    pub fn is_valid_dao_merkle(&self, root: &MerkleNode) -> bool {
        self.dao_roots.iter().any(|m| m == root)
    }

    pub fn is_valid_proposal_merkle(&self, root: &MerkleNode) -> bool {
        self.proposal_roots.iter().any(|m| m == root)
    }
}
