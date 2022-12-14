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

use std::{cmp::Ordering, collections::HashMap, fmt, io};

use async_std::sync::Arc;
use darkfi_serial::{Decodable, Encodable, ReadExt, SerialDecodable, SerialEncodable};
use ripemd::{Digest, Ripemd256};

use darkfi::{Error, Result};

use crate::settings::get_current_time;

pub type EventId = [u8; 32];
pub type EventsQueueArc = Arc<EventsQueue>;

pub struct EventsQueue(smol::channel::Sender<Event>, smol::channel::Receiver<Event>);

impl EventsQueue {
    pub fn new() -> EventsQueueArc {
        let (sn, rv) = smol::channel::unbounded();
        Arc::new(Self(sn, rv))
    }

    pub async fn fetch(&self) -> Result<Event> {
        self.1.recv().await.map_err(Error::from)
    }

    pub async fn dispatch(&self, event: &Event) -> Result<()> {
        self.0.send(event.clone()).await.map_err(Error::from)
    }
}

const MAX_DEPTH: u32 = 300;
const MAX_HEIGHT: u32 = 300;

#[derive(SerialEncodable, SerialDecodable, Clone)]
struct PrivMsgEvent {
    nick: String,
    msg: String,
    target: String,
}

#[derive(Clone)]
enum EventAction {
    PrivMsg(PrivMsgEvent),
}

impl Encodable for EventAction {
    fn encode<S: io::Write>(&self, mut s: S) -> core::result::Result<usize, io::Error> {
        match self {
            Self::PrivMsg(event) => {
                let mut len = 0;
                len += 0u8.encode(&mut s)?;
                len += event.encode(s)?;
                Ok(len)
            }
        }
    }
}

impl Decodable for EventAction {
    fn decode<D: io::Read>(mut d: D) -> core::result::Result<Self, io::Error> {
        let type_id = d.read_u8()?;
        match type_id {
            0 => Ok(Self::PrivMsg(PrivMsgEvent::decode(d)?)),
            _ => Err(io::Error::new(io::ErrorKind::Other, "Bad type ID byte for Event")),
        }
    }
}

#[derive(SerialEncodable, SerialDecodable, Clone)]
pub struct Event {
    previous_event_hash: EventId,
    action: EventAction,
    pub timestamp: u64,
    #[skip_serialize]
    pub read_confirms: u8,
}

impl Event {
    pub fn hash(&self) -> EventId {
        let mut bytes = Vec::new();
        self.encode(&mut bytes).expect("serialize failed!");

        let mut hasher = Ripemd256::new();
        hasher.update(bytes);
        let bytes = hasher.finalize().to_vec();
        let mut result = [0u8; 32];
        result.copy_from_slice(bytes.as_slice());
        result
    }
}

impl fmt::Debug for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.action {
            EventAction::PrivMsg(event) => {
                write!(f, "PRIVMSG {}: {} ({})", event.nick, event.msg, self.timestamp)
            }
        }
    }
}

#[derive(Debug, Clone)]
struct EventNode {
    // Only current root has this set to None
    parent: Option<EventId>,
    event: Event,
    children: Vec<EventId>,
}

pub struct Model {
    // This is periodically updated so we discard old nodes
    current_root: EventId,
    orphans: HashMap<EventId, Event>,
    event_map: HashMap<EventId, EventNode>,
    _events_queue: EventsQueueArc,
}

impl Model {
    pub fn new(events_queue: EventsQueueArc) -> Self {
        let root_node = EventNode {
            parent: None,
            event: Event {
                previous_event_hash: [0u8; 32],
                action: EventAction::PrivMsg(PrivMsgEvent {
                    nick: "root".to_string(),
                    msg: "Let there be dark".to_string(),
                    target: "root".to_string(),
                }),
                timestamp: get_current_time(),
                read_confirms: 0,
            },
            children: Vec::new(),
        };

        let root_node_id = root_node.event.hash();

        let mut event_map = HashMap::new();
        event_map.insert(root_node_id, root_node);

        Self {
            current_root: root_node_id,
            orphans: HashMap::new(),
            event_map,
            _events_queue: events_queue,
        }
    }

    pub fn add(&mut self, event: Event) {
        self.orphans.insert(event.hash(), event);
        self.reorganize();
    }

    pub fn is_orphan(&self, event: &Event) -> bool {
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

    pub fn get_event(&self, event: &EventId) -> Option<Event> {
        self.event_map.get(event).map(|en| en.event.clone())
    }

    pub fn get_event_children(&self, event: &EventId) -> Vec<Event> {
        let mut children = vec![];
        if let Some(ev) = self.event_map.get(event) {
            for child in ev.children.iter() {
                let child = self.event_map.get(child).unwrap();
                children.push(child.event.clone());
            }
        }
        children
    }

    fn reorganize(&mut self) {
        for (_, orphan) in std::mem::take(&mut self.orphans) {
            if self.is_orphan(&orphan) {
                // XXX should we remove orphan if it's too old
                continue
            }

            let prev_event = orphan.previous_event_hash;

            let node =
                EventNode { parent: Some(prev_event), event: orphan.clone(), children: Vec::new() };
            let node_hash = node.event.hash();

            let parent = self.event_map.get_mut(&prev_event).unwrap();
            parent.children.push(node_hash);

            self.event_map.insert(node_hash, node);

            // clean up the tree from old eventnodes
            self.prune_chains();
            self.update_root();
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

    fn update_root(&mut self) {
        let head = self.find_head();
        let leaves = self.find_leaves();

        // find the common ancestor for each leaf and the head event
        let mut ancestors = vec![];
        for leaf in leaves {
            if leaf == head {
                continue
            }

            let ancestor = self.find_ancestor(leaf, head);
            ancestors.push(ancestor);
        }

        // find the highest ancestor
        let highest_ancestor = ancestors
            .iter()
            .max_by(|&a, &b| self.find_depth(*a, &head).cmp(&self.find_depth(*b, &head)));

        // set the new root
        if let Some(ancestor) = highest_ancestor {
            // the ancestor must have at least height > MAX_HEIGHT
            let ancestor_height = self.find_height(&self.current_root, ancestor).unwrap();
            if ancestor_height < MAX_HEIGHT {
                return
            }

            // removing the parents of the new root node
            let mut root = self.event_map.get(&self.current_root).unwrap();
            loop {
                let root_hash = root.event.hash();

                if &root_hash == ancestor {
                    break
                }

                let root_childs = &root.children;
                assert_eq!(root_childs.len(), 1);
                let child = *root_childs.first().unwrap();

                self.event_map.remove(&root_hash);
                root = self.event_map.get(&child).unwrap();
            }

            self.current_root = *ancestor;
        }
    }

    fn remove_node(&mut self, mut event_id: EventId) {
        loop {
            if !self.event_map.contains_key(&event_id) {
                break
            }

            let node = self.event_map.get(&event_id).unwrap().clone();
            self.event_map.remove(&event_id);

            let parent = self.event_map.get_mut(&node.parent.unwrap()).unwrap();
            let index = parent.children.iter().position(|&n| n == event_id).unwrap();
            parent.children.remove(index);

            if !parent.children.is_empty() {
                break
            }
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
                    // Left a todo here because not sure if it should be handled.
                    todo!();
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

    fn find_height(&self, node: &EventId, child_id: &EventId) -> Option<u32> {
        let mut height = 0;

        if node == child_id {
            return Some(height)
        }
        height += 1;
        let children = &self.event_map.get(node).unwrap().children;
        if children.is_empty() {
            return None
        }

        for child in children.iter() {
            if let Some(h) = self.find_height(child, child_id) {
                return Some(height + h)
            }
        }
        None
    }

    fn find_ancestor(&self, mut node_a: EventId, mut node_b: EventId) -> EventId {
        // node_a is a child of node_b
        let is_child = node_b == self.event_map.get(&node_a).unwrap().parent.unwrap();

        if is_child {
            return node_b
        }

        while node_a != node_b {
            let node_a_parent = self.event_map.get(&node_a).unwrap().parent.unwrap();
            let node_b_parent = self.event_map.get(&node_b).unwrap().parent.unwrap();

            if node_a_parent == self.current_root || node_b_parent == self.current_root {
                return self.current_root
            }

            node_a = node_a_parent;
            node_b = node_b_parent;
        }

        node_a
    }

    fn diff_depth(&self, node_a: EventId, node_b: EventId) -> u32 {
        let ancestor = self.find_ancestor(node_a, node_b);
        let node_a_depth = self.find_depth(node_a, &ancestor);
        let node_b_depth = self.find_depth(node_b, &ancestor);

        (node_b_depth + 1) - node_a_depth
    }

    fn _debug(&self) {
        for (event_id, event_node) in &self.event_map {
            let depth = self.find_depth(*event_id, &self.current_root);
            println!("{}: {:?} [depth={}]", hex::encode(event_id), event_node.event, depth);
        }

        println!("root: {}", hex::encode(self.current_root));
        println!("head: {}", hex::encode(self.find_head()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_message(
        previous_event_hash: EventId,
        nick: &str,
        msg: &str,
        timestamp: u64,
    ) -> Event {
        Event {
            previous_event_hash,
            action: EventAction::PrivMsg(PrivMsgEvent {
                nick: nick.to_string(),
                msg: msg.to_string(),
                target: "".to_string(),
            }),
            timestamp,
            read_confirms: 4,
        }
    }

    /* THIS IS FAILING
    #[test]
    fn test_update_root() {
        let events_queue = EventsQueue::new();
        let mut model = Model::new(events_queue);
        let root_id = model.current_root;

        // event_node 1
        // Fill this node with MAX_HEIGHT events
        let mut id1 = root_id;
        for x in 0..MAX_HEIGHT {
            let timestamp = get_current_time() + 1;
            let node = create_message(id1, &format!("chain 1 msg {}", x), "message", timestamp);
            id1 = node.hash();
            model.add(node);
        }

        // event_node 2
        // Fill this node with MAX_HEIGHT + 10 events
        let mut id2 = root_id;
        for x in 0..(MAX_HEIGHT + 10) {
            let timestamp = get_current_time() + 1;
            let node = create_message(id2, &format!("chain 2 msg {}", x), "message", timestamp);
            id2 = node.hash();
            model.add(node);
        }

        // Fill id2 node with MAX_HEIGHT / 2
        let mut id3 = id2;
        for x in (MAX_HEIGHT + 10)..(MAX_HEIGHT * 2) {
            let timestamp = get_current_time() + 1;
            let node =
                create_message(id3, &format!("chain 2 branch 1 msg {}", x), "message", timestamp);
            id3 = node.hash();
            model.add(node);
        }

        // Fill id2 node with 9 events
        let mut id4 = id2;
        for x in (MAX_HEIGHT + 10)..(MAX_HEIGHT * 2 + 30) {
            let timestamp = get_current_time() + 1;
            let node =
                create_message(id4, &format!("chain 2 branch 2 msg {}", x), "message", timestamp);
            id4 = node.hash();
            model.add(node);
        }

        assert_eq!(model.find_height(&model.current_root, &id2).unwrap(), 0);
        assert_eq!(model.find_height(&model.current_root, &id3).unwrap(), (MAX_HEIGHT - 10));
        assert_eq!(model.find_height(&model.current_root, &id4).unwrap(), (MAX_HEIGHT + 20));
        assert_eq!(model.current_root, id2);
    }
    */

    /* THIS IS FAILING
    #[test]
    fn test_find_height() {
        let events_queue = EventsQueue::new();
        let mut model = Model::new(events_queue);
        let root_id = model.current_root;

        // event_node 1
        // Fill this node with 8 events
        let mut id1 = root_id;
        for x in 0..8 {
            let timestamp = get_current_time() + 1;
            let node = create_message(id1, &format!("chain 1 msg {}", x), "message", timestamp);
            id1 = node.hash();
            model.add(node);
        }

        // event_node 2
        // Fill this node with 14 events
        let mut id2 = root_id;
        for x in 0..14 {
            let timestamp = get_current_time() + 1;
            let node = create_message(id2, &format!("chain 2 msg {}", x), "message", timestamp);
            id2 = node.hash();
            model.add(node);
        }

        assert_eq!(model.find_height(&model.current_root, &id1).unwrap(), 8);
        assert_eq!(model.find_height(&model.current_root, &id2).unwrap(), 14);
    }
    */

    /* THIS IS FAILING
    #[test]
    fn test_prune_chains() {
        let events_queue = EventsQueue::new();
        let mut model = Model::new(events_queue);
        let root_id = model.current_root;

        // event_node 1
        // Fill this node with 3 events
        let mut event_node_1_ids = vec![];
        let mut id1 = root_id;
        for x in 0..3 {
            let timestamp = get_current_time() + 1;
            let node = create_message(id1, &format!("chain 1 msg {}", x), "message", timestamp);
            id1 = node.hash();
            model.add(node);
            event_node_1_ids.push(id1);
        }

        // event_node 2
        // Start from the root_id and fill the node with 14 events
        // All the events from event_node_1 should get removed from the tree
        let mut id2 = root_id;
        for x in 0..(MAX_DEPTH + 10) {
            let timestamp = get_current_time() + 1;
            let node = create_message(id2, &format!("chain 2 msg {}", x), "message", timestamp);
            id2 = node.hash();
            model.add(node);
        }

        assert_eq!(model.find_head(), id2);

        for id in event_node_1_ids {
            assert!(!model.event_map.contains_key(&id));
        }

        assert_eq!(model.event_map.len(), (MAX_DEPTH + 11) as usize);
    }
    */

    /* THIS IS FAILING
    #[test]
    fn test_diff_depth() {
        let events_queue = EventsQueue::new();
        let mut model = Model::new(events_queue);
        let root_id = model.current_root;

        // event_node 1
        // Fill this node with (MAX_DEPTH / 2) events
        let mut id1 = root_id;
        for x in 0..(MAX_DEPTH / 2) {
            let timestamp = get_current_time() + 1;
            let node = create_message(id1, &format!("chain 1 msg {}", x), "message", timestamp);
            id1 = node.hash();
            model.add(node);
        }

        // event_node 2
        // Start from the root_id and fill the node with (MAX_DEPTH + 10) events
        // all the events must be added since the depth between id1
        // and the last head is less than MAX_DEPTH
        let mut id2 = root_id;
        for x in 0..(MAX_DEPTH + 10) {
            let timestamp = get_current_time() + 1;
            let node = create_message(id2, &format!("chain 2 msg {}", x), "message", timestamp);
            id2 = node.hash();
            model.add(node);
        }

        assert_eq!(model.find_head(), id2);

        // event_node 3
        // This will start as new chain, but no events will be added
        // since the last event's depth is MAX_DEPTH + 10
        let mut id3 = root_id;
        for x in 0..30 {
            let timestamp = get_current_time() + 1;
            let node = create_message(id3, &format!("chain 3 msg {}", x), "message", timestamp);
            id3 = node.hash();
            model.add(node);

            // ensure events are not added
            assert!(!model.event_map.contains_key(&id3));
        }

        assert_eq!(model.find_head(), id2);

        // Add more events to the event_node 1
        // At the end this chain must overtake the event_node 2
        for x in (MAX_DEPTH / 2)..(MAX_DEPTH + 15) {
            let timestamp = get_current_time() + 1;
            let node = create_message(id1, &format!("chain 1 msg {}", x), "message", timestamp);
            id1 = node.hash();
            model.add(node);
        }

        assert_eq!(model.find_head(), id1);
    }
    */

    #[test]
    fn test_event_hash() {
        let events_queue = EventsQueue::new();
        let model = Model::new(events_queue);
        let root_id = model.current_root;

        let timestamp = get_current_time() + 1;
        let event = create_message(root_id, "msg", "message", timestamp);
        let mut event2 = event.clone();

        let event_hash = event.hash();

        event2.read_confirms += 3;
        let event2_hash = event2.hash();

        assert_eq!(event2_hash, event_hash);
        assert_ne!(event2.read_confirms, event.read_confirms);
    }
}
