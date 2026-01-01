/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
};

use darkfi::util::time::Timestamp;
use darkfi_serial::Encodable;
use num_bigint::BigUint;
use rand::prelude::SliceRandom;

/// Number of random samples in each query
const K: usize = 20;
/// Minimum number of votes to count as a successful query
const ALPHA: usize = 14;
/// Consecutive successful queries required for consensus
const BETA: usize = 20;

// Security and network dynamics related constants

/// Amount of nodes in the created network
const NETWORK_SIZE: usize = 100;
// A node can produce a max of 10 messages per cycle
//const MAX_MESSAGE_RATE: usize = 10;
/// A node that produces >5 malformed messages is considered malicious
const MALICIOUS_THRESHOLD: usize = 5;
/// A node only gossips to 10 random peers
const GOSSIP_SIZE: usize = 10;
/// 2% probability that a node goes offline
const NODE_OFFLINE_PROB: f64 = 0.02;
/// 5% probability that a node comes back online
const NODE_ONLINE_PROB: f64 = 0.05;
/// 5% probability that a node becomes malicious
const NODE_MALICIOUS_PROB: f64 = 0.05;
/// Maximum storage capacity for each node in terms of number of messages
const MAX_STORAGE_CAPACITY: usize = 500;

struct Metrics {
    offline_nodes: Vec<usize>,
    malicious_nodes: Vec<usize>,
    malformed_messages: usize,
    messages_stored: HashMap<usize, usize>,
}

impl Metrics {
    fn new() -> Self {
        Metrics {
            offline_nodes: vec![],
            malicious_nodes: vec![],
            malformed_messages: 0,
            messages_stored: HashMap::new(),
        }
    }

    // Utility functions to update metrics
    fn increment_malformed(&mut self) {
        self.malformed_messages += 1;
    }

    fn update_stored_messages(&mut self, node_id: usize, count: usize) {
        self.messages_stored.insert(node_id, count);
    }
}

#[derive(Hash, Clone, Eq, PartialEq, Debug)]
struct Message {
    timestamp: Timestamp,
    content: String,
    // The IDs of previous messages
    references: Vec<blake3::Hash>,
}

impl Message {
    fn id(&self) -> blake3::Hash {
        let mut hasher = blake3::Hasher::new();
        self.timestamp.inner().encode(&mut hasher).unwrap();
        self.content.encode(&mut hasher).unwrap();
        for reference in &self.references {
            reference.as_bytes().encode(&mut hasher).unwrap();
        }
        hasher.finalize()
    }
}

#[derive(Clone, Eq, PartialEq)]
struct SnowballNode {
    id: usize,
    malicious_counter: usize,
    online: bool,
    malicious: bool,
    preference: Option<Message>,
    message_votes: HashMap<Message, usize>,
    counts: HashMap<Message, usize>,
    dag: HashMap<blake3::Hash, Message>,
    orphan_pool: Vec<Message>,
    finalized_preference: Option<Message>,
}

impl std::hash::Hash for SnowballNode {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl SnowballNode {
    fn new(id: usize) -> Self {
        SnowballNode {
            id,
            malicious_counter: 0,
            online: true,
            malicious: false,
            preference: None,
            message_votes: HashMap::new(),
            counts: HashMap::new(),
            dag: HashMap::new(),
            orphan_pool: vec![],
            finalized_preference: None,
        }
    }

    fn is_malicious(&self) -> bool {
        self.malicious || self.malicious_counter > MALICIOUS_THRESHOLD
    }

    fn query(&self, network: &HashMap<usize, SnowballNode>) -> Option<Message> {
        let mut sample_votes = HashMap::new();

        for _ in 0..K {
            // Get a random node
            let node = &network[&(rand::random::<usize>() % NETWORK_SIZE)];
            if let Some(pref) = &node.preference {
                *sample_votes.entry(pref.clone()).or_insert(0) += 1;
            }
        }

        sample_votes.into_iter().max_by_key(|&(_, count)| count).map(|(message, _)| message)
    }

    fn receive_vote(&mut self, from_node: &SnowballNode, message: &Message) {
        // Request any missing parent messages
        let missing_parents = self.request_missing_references(from_node, message);
        for parent_msg in missing_parents {
            if self.validate_message(&parent_msg) {
                self.add_to_dag(&parent_msg);
            } else {
                // The parent message is invalid.
                self.malicious_counter += 1;
                return
            }
        }

        *self.message_votes.entry(message.clone()).or_insert(0) += 1;

        if message.references.iter().all(|ref_id| self.dag.contains_key(ref_id)) {
            self.add_to_dag(message);
        } else {
            self.orphan_pool.push(message.clone());
        }
    }

    fn request_missing_references(
        &self,
        from_node: &SnowballNode,
        message: &Message,
    ) -> Vec<Message> {
        let mut missing_refs = Vec::new();

        for ref_id in &message.references {
            if !self.dag.contains_key(ref_id) {
                if let Some(parent_msg) = from_node.dag.get(ref_id) {
                    missing_refs.push(parent_msg.clone());
                }
            }
        }

        missing_refs
    }

    fn update_preference(&mut self) {
        let mut max_message = None;
        let mut max_count: usize = 0;
        let mut max_timestamp = Timestamp::current_time();
        let mut max_target = BigUint::from_bytes_be(&[0xff; 32]);

        for (message, &vote_count) in self.message_votes.iter() {
            let is_better = match vote_count.cmp(&max_count) {
                Ordering::Greater => true,
                Ordering::Equal => match message.timestamp.cmp(&max_timestamp) {
                    Ordering::Less => true,
                    Ordering::Equal => {
                        let message_target = BigUint::from_bytes_be(message.id().as_bytes());
                        message_target < max_target
                    }
                    Ordering::Greater => false,
                },
                Ordering::Less => false,
            };

            if is_better {
                max_count = vote_count;
                max_message = Some(message.clone());
                max_timestamp = message.timestamp;
                max_target = BigUint::from_bytes_be(message.id().as_bytes());
            }
        }

        if let Some(max_message) = max_message {
            if max_count >= ALPHA {
                *self.counts.entry(max_message.clone()).or_insert(0) += 1;

                if self.counts[&max_message] >= BETA {
                    // Setting the finalized preference if not already set
                    if self.finalized_preference.is_none() {
                        self.finalized_preference = Some(max_message.clone());
                        //println!(
                        //    "Node {} finalized preference to message {}",
                        //    self.id, max_message.content
                        //);
                    }
                    self.preference = Some(max_message);
                }
            } else {
                self.counts.insert(max_message, 0);
            }
        }
    }

    fn add_to_dag(&mut self, msg: &Message) {
        self.dag.insert(msg.id(), msg.clone());
        self.check_orphan_pool();
    }

    fn check_orphan_pool(&mut self) {
        let mut i = 0;
        while i < self.orphan_pool.len() {
            if self.orphan_pool[i].references.iter().all(|ref_id| self.dag.contains_key(ref_id)) {
                let msg = self.orphan_pool.remove(i);
                self.add_to_dag(&msg);
            } else {
                i += 1;
            }
        }
    }

    fn random_references(&self) -> Vec<blake3::Hash> {
        let mut references = vec![];
        let keys: Vec<blake3::Hash> = self.dag.keys().cloned().collect();
        if !keys.is_empty() {
            // Up to 2 references
            for _ in 0..rand::random::<usize>() % 3 {
                let random_ref = keys[rand::random::<usize>() % keys.len()];
                if !references.contains(&random_ref) {
                    references.push(random_ref);
                }
            }
        }

        references
    }

    fn act_malicious(&mut self, network: &HashMap<usize, SnowballNode>) -> Option<Message> {
        if rand::random::<f64>() < 0.7 {
            // 70% chance to send a malformed message
            let references = self.random_references();
            let malformed_msg = Message {
                timestamp: Timestamp::current_time(),
                content: format!("Malformed {}", rand::random::<usize>() % 1000),
                references,
            };
            return Some(malformed_msg)
        } else {
            // 30% chance to change preference rapidly
            if let Some(vote) = self.query(network) {
                self.preference = Some(vote);
            }
        }
        None
    }

    fn validate_message(&self, message: &Message) -> bool {
        // In our example, simply checking if the content starts with "Malformed"
        !message.content.starts_with("Malformed")
    }

    fn prune_old_messages(&mut self) {
        if self.dag.len() > MAX_STORAGE_CAPACITY {
            // Here we're just removing random messages, but in a real-world application,
            // more sophisticated policies would be needed.
            let random_key =
                *self.dag.keys().nth(rand::random::<usize>() % self.dag.len()).unwrap();
            self.dag.remove(&random_key);
        }
    }
}

fn main() {
    let mut network: HashMap<usize, SnowballNode> = HashMap::new();
    let mut offline_nodes: HashSet<usize> = HashSet::new();

    let mut metrics = Metrics::new();

    // Genesis message
    let genesis = Message {
        timestamp: Timestamp::current_time(),
        content: String::from("Genesis"),
        references: vec![],
    };

    // Initialize nodes and add the genesis message to each node's DAG
    for i in 0..NETWORK_SIZE {
        let mut node = SnowballNode::new(i);
        node.add_to_dag(&genesis);
        node.online = rand::random::<f64>() < NODE_ONLINE_PROB;
        node.malicious = rand::random::<f64>() < NODE_MALICIOUS_PROB;

        network.insert(i, node);
    }

    for _ in 0..1000 {
        // Simulate network dynamics
        for idx in 0..NETWORK_SIZE {
            if rand::random::<f64>() < NODE_OFFLINE_PROB && !offline_nodes.contains(&idx) {
                offline_nodes.insert(idx);
                network.get_mut(&idx).unwrap().online = false;
                //println!("Node {} went offline", idx);
            } else if rand::random::<f64>() < NODE_ONLINE_PROB && offline_nodes.contains(&idx) {
                offline_nodes.remove(&idx);
                network.get_mut(&idx).unwrap().online = true;
                //println!("Node {} came online", idx);
            }
        }

        metrics.offline_nodes.push(offline_nodes.len());
        metrics
            .malicious_nodes
            .push(network.iter().filter(|(_, node)| node.is_malicious()).count());

        // This simulates concurrent conflicting messages being sent
        // Up to 5 nodes may produce messages concurrently:
        let number_of_messages = rand::random::<usize>() % 5;
        for _ in 0..number_of_messages {
            let random_node_index = rand::random::<usize>() % NETWORK_SIZE;
            if let Some(node) = network.get_mut(&random_node_index) {
                if !node.is_malicious() && node.online {
                    //println!("Node {} created a message", random_node_index);
                    let references = node.random_references();
                    let msg = Message {
                        timestamp: Timestamp::current_time(),
                        content: format!("Message {}", rand::random::<usize>() % 1000),
                        references,
                    };
                    node.add_to_dag(&msg);
                    node.preference = Some(msg.clone());
                }
            }
        }

        // Nodes may act maliciously
        for node in network.clone().values_mut() {
            if node.is_malicious() && node.online {
                if let Some(malformed_msg) = node.act_malicious(&network) {
                    // Disseminate the malformed message
                    let mut node_indices: Vec<usize> = network.keys().cloned().collect();
                    node_indices.shuffle(&mut rand::thread_rng());
                    for &idx in node_indices.iter().take(GOSSIP_SIZE) {
                        if let Some(other_node) = network.get_mut(&idx) {
                            if !other_node.is_malicious() && other_node.online {
                                other_node.receive_vote(node, &malformed_msg);
                            }
                        }
                    }

                    metrics.increment_malformed();
                }
            }
        }

        for node in network.clone().values() {
            if node.online {
                if let Some(vote) = node.query(&network) {
                    // Add random delay before disseminating
                    //std::thread::sleep(std::time::Duration::from_millis(rand::random::<u64>() % 100));
                    let mut node_indices: Vec<usize> = network.keys().cloned().collect();
                    node_indices.shuffle(&mut rand::thread_rng());
                    // Implementing gossip protocol
                    for &idx in node_indices.iter().take(GOSSIP_SIZE) {
                        if let Some(other_node) = network.get_mut(&idx) {
                            if other_node.validate_message(&vote) {
                                if !other_node.is_malicious() && other_node.online {
                                    other_node.receive_vote(node, &vote);
                                }
                            } else {
                                // Increase malicious counter if a malformed message is received
                                other_node.malicious_counter += 1;
                            }
                        }
                    }
                }
            }
        }

        for node in network.values_mut() {
            if node.online {
                node.update_preference();
            }
            node.prune_old_messages();
            metrics.update_stored_messages(node.id, node.dag.len());
        }
    }

    // Check the state of the network
    let consensus_count = network.iter().filter(|(_, node)| node.preference.is_some()).count();
    println!("Number of nodes that reached consensus: {}", consensus_count);

    let finalized_count =
        network.iter().filter(|(_, node)| node.finalized_preference.is_some()).count();
    println!("Number of nodes that reached explicit finality: {}", finalized_count);

    //println!("Total malformed messages detected: {}", metrics.malformed_messages);
    //println!("Malicious nodes per cycle: {:?}", metrics.malicious_nodes);
    //println!("Offline nodes per cycle: {:?}", metrics.offline_nodes);
    //println!("Messages stored by node per cycle: {:?}", metrics.messages_stored);
}
