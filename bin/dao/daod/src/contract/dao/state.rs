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

use std::{any::Any, collections::HashMap};

use darkfi_sdk::crypto::{constants::MERKLE_DEPTH, MerkleNode, Nullifier};
use darkfi_serial::{SerialDecodable, SerialEncodable};
use incrementalmerkletree::{bridgetree::BridgeTree, Tree};
use pasta_curves::{
    group::{ff::PrimeField, Group},
    pallas,
};

#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct DaoBulla(pub pallas::Base);

type MerkleTree = BridgeTree<MerkleNode, MERKLE_DEPTH>;

pub struct ProposalVotes {
    // TODO: might be more logical to have 'yes_votes_commit' and 'no_votes_commit'
    /// Weighted vote commit
    pub yes_votes_commit: pallas::Point,
    /// All value staked in the vote
    pub all_votes_commit: pallas::Point,
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
    pub proposal_votes: HashMap<[u8; 32], ProposalVotes>,
}

impl State {
    pub fn new() -> Box<dyn Any + Send> {
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
        let node = MerkleNode::from(bulla.0);
        self.dao_bullas.push(bulla);
        self.dao_tree.append(&node);
        self.dao_roots.push(self.dao_tree.root(0).unwrap());
    }

    pub fn add_proposal_bulla(&mut self, bulla: pallas::Base) {
        let node = MerkleNode::from(bulla);
        //self.proposal_bullas.push(bulla);
        self.proposal_tree.append(&node);
        self.proposal_roots.push(self.proposal_tree.root(0).unwrap());
        self.proposal_votes.insert(
            bulla.to_repr(),
            ProposalVotes {
                yes_votes_commit: pallas::Point::identity(),
                all_votes_commit: pallas::Point::identity(),
                vote_nulls: Vec::new(),
            },
        );
    }

    pub fn lookup_proposal_votes(&self, proposal_bulla: pallas::Base) -> Option<&ProposalVotes> {
        self.proposal_votes.get(&proposal_bulla.to_repr())
    }
    pub fn lookup_proposal_votes_mut(
        &mut self,
        proposal_bulla: pallas::Base,
    ) -> Option<&mut ProposalVotes> {
        self.proposal_votes.get_mut(&proposal_bulla.to_repr())
    }

    pub fn is_valid_dao_merkle(&self, root: &MerkleNode) -> bool {
        self.dao_roots.iter().any(|m| m == root)
    }

    // TODO: This never gets called.
    pub fn _is_valid_proposal_merkle(&self, root: &MerkleNode) -> bool {
        self.proposal_roots.iter().any(|m| m == root)
    }
}
