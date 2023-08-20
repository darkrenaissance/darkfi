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

use std::{cmp::Ordering, collections::HashMap, fmt::Debug, path::Path};

use async_std::sync::{Arc, Mutex};
use blake3;
use darkfi_serial::{
    deserialize, serialize, Decodable, Encodable, SerialDecodable, SerialEncodable,
};
use log::{error, info};
use tinyjson::JsonValue;

use crate::{
    event_graph::events_queue::EventsQueuePtr,
    util::{
        encoding::base64,
        file::{load_json_file, save_json_file},
        time::Timestamp,
    },
};

use super::EventMsg;

//pub type EventId = [u8; blake3::OUT_LEN];
pub type EventId = blake3::Hash;

const MAX_DEPTH: u32 = 300;

#[derive(SerialEncodable, SerialDecodable, Clone, Debug)]
pub struct Event<T: Send + Sync> {
    pub previous_event_hash: EventId,
    pub action: T,
    pub timestamp: Timestamp,
}

impl<T> Event<T>
where
    T: Send + Sync + Encodable + Decodable + Clone + EventMsg,
{
    pub fn hash(&self) -> EventId {
        blake3::hash(&serialize(self))
    }
}

#[derive(SerialEncodable, SerialDecodable, Clone, Debug)]
struct EventNode<T: Send + Sync> {
    // Only current root has this set to None
    parent: Option<EventId>,
    event: Event<T>,
    children: Vec<EventId>,
}

pub type ModelPtr<T> = Arc<Mutex<Model<T>>>;

pub struct Model<T: Send + Sync + Debug> {
    // This is up to the application to reset or keep
    current_root: EventId,
    orphans: HashMap<EventId, Event<T>>,
    event_map: HashMap<EventId, EventNode<T>>,
    events_queue: EventsQueuePtr<T>,
}

impl<T> Model<T>
where
    T: Send + Sync + Encodable + Decodable + Clone + EventMsg + Debug,
{
    pub fn new(events_queue: EventsQueuePtr<T>) -> Self {
        let root_node = EventNode {
            parent: None,
            event: Event {
                previous_event_hash: blake3::hash(b""), // This is a blake3 hash of NULL
                action: T::new(),
                timestamp: Timestamp(1674512021323),
            },
            children: Vec::new(),
        };

        let root_node_id = root_node.event.hash();

        let mut event_map = HashMap::new();
        event_map.insert(root_node_id, root_node);

        Self { current_root: root_node_id, orphans: HashMap::new(), event_map, events_queue }
    }

    pub fn save_tree(&self, path: &Path) -> crate::Result<()> {
        let path = path.join("tree");
        let tree = self.event_map.clone();
        let ser_tree = base64::encode(&serialize(&tree));

        save_json_file(&path, &JsonValue::String(ser_tree), false)?;

        info!("Tree is saved to disk");

        Ok(())
    }

    pub fn load_tree(&mut self, path: &Path) -> crate::Result<()> {
        let path = path.join("tree");
        if !path.exists() {
            return Ok(())
        }

        let loaded_tree_obj = load_json_file(&path)?;
        let loaded_tree_obj: &String = loaded_tree_obj.get::<String>().unwrap();
        let loaded_tree_bytes = base64::decode(loaded_tree_obj.as_str()).unwrap();
        let dser_tree: HashMap<blake3::Hash, EventNode<T>> = deserialize(&loaded_tree_bytes)?;
        self.event_map = dser_tree;

        info!("Tree is loaded from disk");

        Ok(())
    }

    pub fn reset_root(&mut self, timestamp: Timestamp) {
        let root_node = EventNode {
            parent: None,
            event: Event {
                previous_event_hash: blake3::hash(b""), // This is a blake3 hash of NULL
                action: T::new(),
                timestamp,
            },
            children: Vec::new(),
        };

        let root_node_id = root_node.event.hash();

        let mut event_map = HashMap::new();
        event_map.insert(root_node_id, root_node);

        self.current_root = root_node_id;
        self.orphans = HashMap::new();
        self.event_map = event_map;

        info!("reset current root to: {:?}", self.current_root);
    }

    pub fn remove_old_events(&mut self, timestamp: Timestamp) -> crate::Result<()> {
        let tree = self.event_map.clone();
        let mut is_tree_changed = false;
        for (event_hash, node) in tree {
            if node.event.timestamp < timestamp {
                if self.event_map.remove(&event_hash).is_none() {
                    continue
                }
                is_tree_changed = true;
                let parent = self.event_map.get_mut(&self.current_root).unwrap();
                if parent.children.contains(&event_hash) {
                    let index = parent.children.iter().position(|&n| n == event_hash).unwrap();
                    parent.children.remove(index);
                }
            }
        }
        if is_tree_changed {
            let binding = self.event_map.clone();
            let min_hash = binding.iter().min_by_key(|entry| entry.1.event.timestamp.0).unwrap().0;

            println!("min hash: {}", min_hash);

            self.event_map.get_mut(min_hash).unwrap().parent = Some(self.current_root);
            self.event_map.get_mut(min_hash).unwrap().event.previous_event_hash = self.current_root;

            let parent = self.event_map.get_mut(&self.current_root).unwrap();
            parent.children.push(*min_hash);
        }

        Ok(())
    }

    pub fn get_head_hash(&self) -> EventId {
        self.find_head()
    }

    pub async fn add(&mut self, event: Event<T>) {
        self.orphans.insert(event.hash(), event);
        self.reorganize().await;
    }

    pub fn is_orphan(&self, event: &Event<T>) -> bool {
        !self.event_map.contains_key(&event.previous_event_hash)
    }

    pub fn find_leaves(&self) -> Vec<EventId> {
        // collect the leaves in the tree
        let mut leaves = vec![];

        for (event_hash, node) in self.event_map.iter() {
            // check if the node is a leaf
            if node.children.is_empty() {
                leaves.push(*event_hash);
            }
        }

        leaves
    }

    pub fn get_event(&self, event: &EventId) -> Option<Event<T>> {
        self.event_map.get(event).map(|en| en.event.clone())
    }

    pub fn get_offspring(&self, event: &EventId) -> Vec<Event<T>> {
        let mut offspring = vec![];
        let mut event = *event;
        let head = self.find_head();
        loop {
            if event == head {
                break
            }
            if let Some(ev) = self.event_map.get(&event) {
                for child in ev.children.iter() {
                    let child = self.event_map.get(child).unwrap();
                    offspring.push(child.event.clone());
                    event = child.event.hash();
                }
            } else {
                break
            }
        }

        offspring
    }

    async fn reorganize(&mut self) {
        for (_, orphan) in std::mem::take(&mut self.orphans) {
            // if self.is_orphan(&orphan) {
            //     // TODO should we remove orphan if it's too old
            //     continue
            // }

            let prev_event = orphan.previous_event_hash;

            let node =
                EventNode { parent: Some(prev_event), event: orphan.clone(), children: Vec::new() };
            let node_hash = node.event.hash();

            let parent = match self.event_map.get_mut(&prev_event) {
                Some(parent) => parent,
                None => {
                    error!("No parent found, Orphan is not relinked");
                    self.orphans.insert(orphan.hash(), orphan);
                    continue
                }
            };
            parent.children.push(node_hash);

            self.event_map.insert(node_hash, node.clone());

            self.events_queue.dispatch(&node.event).await.ok();

            // clean up the tree from old eventnodes
            self.prune_chains();
        }
    }

    fn prune_chains(&mut self) {
        let head = self.find_head();
        let leaves = self.find_leaves();

        // Reject events which attach to chains too low in the chain
        // At some point we ignore all events from old branches
        for leaf in leaves {
            // skip the head event
            if leaf == head {
                continue
            }

            let depth = self.diff_depth(leaf, head);
            if depth > MAX_DEPTH {
                self.remove_node(leaf);
            }
        }
    }

    fn remove_node(&mut self, mut event_id: EventId) {
        loop {
            if !self.event_map.contains_key(&event_id) {
                break
            }

            if event_id == self.current_root {
                break
            }

            let node = self.event_map.get(&event_id).unwrap().clone();
            self.event_map.remove(&event_id);

            let parent = self.event_map.get_mut(&node.parent.unwrap()).unwrap();

            if parent.children.is_empty() {
                event_id = parent.event.hash();
                continue
            }
            let index = parent.children.iter().position(|&n| n == event_id).unwrap();
            parent.children.remove(index);

            event_id = parent.event.hash();
        }
    }

    // find_head
    // -> recursively call itself
    // -> + 1 for every recursion, return self if no children
    // -> select max from returned values
    // Gets the lead node with the maximal number of events counting from root
    fn find_head(&self) -> EventId {
        self.find_longest_chain(&self.current_root, 0).0
    }

    fn find_longest_chain(&self, parent_node: &EventId, i: u32) -> (EventId, u32) {
        let children = &self.event_map.get(parent_node).unwrap().children;
        if children.is_empty() {
            return (*parent_node, i)
        }

        let mut current_max = 0;
        let mut current_node = None;
        for node in children.iter() {
            let (grandchild_node, grandchild_i) = self.find_longest_chain(node, i + 1);

            match &grandchild_i.cmp(&current_max) {
                Ordering::Greater => {
                    current_max = grandchild_i;
                    current_node = Some(grandchild_node);
                }
                Ordering::Equal => {
                    // Break ties using the timestamp
                    let grandchild_node_timestamp =
                        self.event_map.get(&grandchild_node).unwrap().event.timestamp;
                    let current_node_timestamp =
                        self.event_map.get(&current_node.unwrap()).unwrap().event.timestamp;

                    if grandchild_node_timestamp > current_node_timestamp {
                        current_max = grandchild_i;
                        current_node = Some(grandchild_node);
                    }
                }
                Ordering::Less => {
                    // Left a todo here, not sure if it should be handled
                    continue
                }
            }
        }
        assert_ne!(current_max, 0);
        (current_node.expect("internal logic error"), current_max)
    }

    fn find_depth(&self, mut node: EventId, ancestor_id: &EventId) -> u32 {
        let mut depth = 0;
        while &node != ancestor_id {
            depth += 1;
            if let Some(parent) = self.event_map.get(&node).unwrap().parent {
                node = parent
            } else {
                break
            }
        }
        depth
    }

    // Find common ancestor between two events
    fn find_ancestor(&self, mut node_a: EventId, node_b: EventId) -> EventId {
        // node_a is a child of node_b
        let is_child = node_b == self.event_map.get(&node_a).unwrap().parent.unwrap();
        if is_child {
            return node_b
        }

        loop {
            let node_a_parent = self.event_map.get(&node_a).unwrap().parent.unwrap();
            node_a = node_a_parent;
            if node_a == self.current_root {
                return self.current_root
            }
            if self.event_map.get(&node_a).unwrap().children.len() > 1 {
                let offsprings = self
                    .get_offspring(&node_a)
                    .iter()
                    .map(|event| event.hash())
                    .collect::<Vec<EventId>>();
                if offsprings.contains(&node_b) {
                    return node_a
                }
            }
        }
    }

    // Find the length between two events
    fn diff_depth(&self, node_a: EventId, node_b: EventId) -> u32 {
        let ancestor = self.find_ancestor(node_a, node_b);
        let node_a_depth = self.find_depth(node_a, &ancestor);
        let node_b_depth = self.find_depth(node_b, &ancestor);

        (node_b_depth + 1).abs_diff(node_a_depth)
    }

    fn _debug(&self) {
        for (event_id, event_node) in &self.event_map {
            let depth = self.find_depth(*event_id, &self.current_root);
            println!("{}: {:?} [depth={}]", event_id, event_node.event, depth);
        }

        println!("root: {}", self.current_root);
        println!("head: {}", self.find_head());
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{create_dir_all, remove_dir_all},
        path::PathBuf,
    };

    use super::*;
    use crate::{event_graph::events_queue::EventsQueue, util::async_util::sleep, Result};

    #[derive(SerialEncodable, SerialDecodable, Clone, Debug)]
    pub struct PrivMsgEvent {
        pub nick: String,
        pub msg: String,
        pub target: String,
    }

    impl std::string::ToString for PrivMsgEvent {
        fn to_string(&self) -> String {
            format!(":{}!anon@dark.fi PRIVMSG {} :{}\r\n", self.nick, self.target, self.msg)
        }
    }

    impl EventMsg for PrivMsgEvent {
        fn new() -> Self {
            Self {
                nick: "root".to_string(),
                msg: "Let there be dark".to_string(),
                target: "root".to_string(),
            }
        }
    }

    fn create_message(previous_event_hash: EventId, timestamp: Timestamp) -> Event<PrivMsgEvent> {
        Event { previous_event_hash, action: PrivMsgEvent::new(), timestamp }
    }

    #[async_std::test]
    async fn test_remove_old_events() {
        let events_queue = EventsQueue::new();
        let mut model = Model::new(events_queue);
        let root_id = model.current_root;

        // event_node 1
        // Fill this node with 10 events
        // These are considered old events from 10 days ago
        let mut event_node_1_ids = vec![];
        let mut id1 = root_id;
        let timestamp = Timestamp::current_time().0 - 864000; // 864000 is 10 days in seconds
        for i in 0..10 {
            let node = create_message(id1, Timestamp(timestamp + i));
            id1 = node.hash();
            model.add(node).await;
            event_node_1_ids.push(id1);
        }
        sleep(1).await;

        // event_node 2
        // Fill this node with 10 events
        // These are considered new events at current time
        let timestamp = Timestamp::current_time().0;
        for i in 0..150 {
            let node = create_message(id1, Timestamp(timestamp + i));
            id1 = node.hash();
            model.add(node).await;
        }
        sleep(1).await;

        // every event older than one week gets removed
        let ts = Timestamp::current_time().0 - 604800; // one week in seconds
        let _ = model.remove_old_events(Timestamp(ts));

        // ensure the 10 events from event_node 1 are not in the tree anymore
        for event in event_node_1_ids {
            assert!(!model.event_map.contains_key(&event));
        }

        // event_node 2 events (150) + root event = 151 events
        assert_eq!(model.event_map.len(), 151_usize);
    }

    #[async_std::test]
    async fn test_prune_chains() {
        let events_queue = EventsQueue::new();
        let mut model = Model::new(events_queue);
        let root_id = model.current_root;

        // event_node 1
        // Fill this node with 10 events
        let mut event_node_1_ids = vec![];
        let mut id1 = root_id;
        for _ in 0..10 {
            let node = create_message(id1, Timestamp::current_time());
            id1 = node.hash();
            model.add(node).await;
            event_node_1_ids.push(id1);
        }

        sleep(1).await;

        // event_node 2
        // Start from the root_id and fill the node with (MAX_DEPTH + 10) events.
        // All the events from event_node_1 should get removed from the tree
        let mut id2 = root_id;
        for _ in 0..(MAX_DEPTH + 10) {
            let node = create_message(id2, Timestamp::current_time());
            id2 = node.hash();
            model.add(node).await;
        }

        assert_eq!(model.find_head(), id2);

        // Ensure events from node 1 are removed in favor of node 2's longer chain
        for id in event_node_1_ids {
            assert!(!model.event_map.contains_key(&id));
        }

        // node1: (10 leaves) + node2: (MAX_DEPTH + 10) events + root event = (MAX_DEPTH + 11)
        //  these ^^^^^^^^^^^ are pruned
        assert_eq!(model.event_map.len(), (MAX_DEPTH + 11) as usize);
    }

    #[async_std::test]
    async fn test_diff_depth() {
        let events_queue = EventsQueue::new();
        let mut model = Model::new(events_queue);
        let root_id = model.current_root;

        // event_node 1
        // Fill this node with (MAX_DEPTH / 2) events
        let mut id1 = root_id;
        for _ in 0..(MAX_DEPTH / 2) {
            let node = create_message(id1, Timestamp::current_time());
            id1 = node.hash();
            model.add(node).await;
        }

        sleep(1).await;

        // event_node 2
        // Start from the root_id and fill the node with (MAX_DEPTH + 10) events
        // all the events must be added since the depth between id1
        // and the last head is less than MAX_DEPTH
        let mut id2 = root_id;
        for _ in 0..(MAX_DEPTH + 10) {
            let node = create_message(id2, Timestamp::current_time());
            id2 = node.hash();
            model.add(node).await;
        }

        assert_eq!(model.find_head(), id2);

        sleep(1).await;

        // event_node 3
        // This will start as new chain, but no events will be added
        // since the last event's depth is MAX_DEPTH + 10
        let mut id3 = root_id;
        for _ in 0..30 {
            let node = create_message(id3, Timestamp::current_time());
            id3 = node.hash();
            model.add(node).await;

            // ensure events are not added
            assert!(!model.event_map.contains_key(&id3));
        }

        sleep(1).await;

        assert_eq!(model.find_head(), id2);

        // Add more events to the event_node 1
        // At the end this chain must overtake the event_node 2
        for _ in (MAX_DEPTH / 2)..(MAX_DEPTH + 15) {
            let node = create_message(id1, Timestamp::current_time());
            id1 = node.hash();
            model.add(node).await;
        }

        assert_eq!(model.find_head(), id1);
    }

    #[async_std::test]
    async fn save_load_model() -> Result<()> {
        // Setup directories
        let path = "/tmp/test_model";
        remove_dir_all(path).ok();
        let path = PathBuf::from(path);
        create_dir_all(&path)?;

        // First model
        let events_queue = EventsQueue::<PrivMsgEvent>::new();
        let mut model1 = Model::new(events_queue);
        let root_id = model1.current_root;

        // Create an event
        let event = create_message(root_id, Timestamp::current_time());
        // Add event to first model
        model1.add(event).await;

        // Save first model
        model1.save_tree(&path)?;

        // Second model
        let events_queue = EventsQueue::<PrivMsgEvent>::new();
        let mut model2 = Model::new(events_queue);

        // Load into second model
        model2.load_tree(&path)?;

        // Test equality
        let res = model1.event_map.len() == model2.event_map.len() &&
            model1.event_map.keys().all(|k| model2.event_map.contains_key(k));

        assert!(res);

        remove_dir_all(path).ok();

        Ok(())
    }

    #[test]
    fn test_event_hash() {
        let events_queue = EventsQueue::<PrivMsgEvent>::new();
        let model = Model::new(events_queue);
        let root_id = model.current_root;

        let event = create_message(root_id, Timestamp::current_time());
        let event2 = event.clone();

        let event_hash = event.hash();

        let event2_hash = event2.hash();

        assert_eq!(event2_hash, event_hash);
    }
}
