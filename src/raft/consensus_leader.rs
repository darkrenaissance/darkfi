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
use std::collections::HashMap;

use darkfi_serial::{serialize, Decodable, Encodable};

use crate::Result;

use super::{
    primitives::{LogRequest, LogResponse, Logs, NetMsgMethod, NodeId, Role},
    Raft,
};

impl<T: Decodable + Encodable + Clone> Raft<T> {
    pub(super) async fn send_heartbeat(&mut self) -> Result<()> {
        if self.role != Role::Leader {
            return Ok(())
        }

        let nodes = self.nodes.lock().await;
        let nodes_cloned = nodes.clone();
        drop(nodes);
        for node in nodes_cloned.iter() {
            self.update_logs(node.0).await?;
        }
        Ok(())
    }

    async fn update_logs(&mut self, node_id: &NodeId) -> Result<()> {
        let prefix_len = match self.sent_length.get(node_id) {
            Ok(len) => len,
            Err(_) => {
                self.sent_length.insert(node_id, 0);
                self.acked_length.insert(node_id, 0);
                0
            }
        };

        let suffix: Logs = match self.slice_logs_from(prefix_len)? {
            Some(l) => l,
            None => return Ok(()),
        };

        let mut prefix_term = 0;

        if prefix_len > 0 {
            prefix_term = self.get_log(prefix_len - 1)?.term;
        }

        let request = LogRequest {
            leader_id: self.id(),
            current_term: self.current_term()?,
            prefix_len,
            prefix_term,
            commit_length: self.commits_len(),
            suffix,
        };

        let payload = serialize(&request);
        self.send(Some(node_id.clone()), &payload, NetMsgMethod::LogRequest, None).await
    }

    pub(super) async fn receive_log_response(&mut self, lr: LogResponse) -> Result<()> {
        if lr.current_term == self.current_term()? && self.role == Role::Leader {
            if lr.ok && lr.ack >= self.acked_length.get(&lr.node_id)? {
                self.sent_length.insert(&lr.node_id, lr.ack);
                self.acked_length.insert(&lr.node_id, lr.ack);
                self.commit_log().await?;
            } else if self.sent_length.get(&lr.node_id)? > 0 {
                self.sent_length.insert(&lr.node_id, self.sent_length.get(&lr.node_id)? - 1);
            }
        } else if lr.current_term > self.current_term()? {
            self.set_current_term(&lr.current_term)?;
            self.role = Role::Follower;
            self.set_voted_for(&None)?;
        }

        Ok(())
    }

    fn acks(&self, nodes: HashMap<NodeId, i64>, length: u64) -> HashMap<NodeId, i64> {
        nodes
            .into_iter()
            .filter(|n| {
                let len = self.acked_length.get(&n.0);
                len.is_ok() && len.unwrap() >= length
            })
            .collect()
    }

    async fn commit_log(&mut self) -> Result<()> {
        let nodes_ptr = self.nodes.lock().await;
        let min_acks = ((nodes_ptr.len() + 1) / 2) as usize;
        let nodes = nodes_ptr.clone();
        drop(nodes_ptr);

        let mut ready: Vec<u64> = vec![];

        for len in 1..(self.logs_len() + 1) {
            if self.acks(nodes.clone(), len).len() >= min_acks {
                ready.push(len);
            }
        }

        if ready.is_empty() {
            return Ok(())
        }

        let max_ready = *ready.iter().max().unwrap();

        if max_ready > self.commits_len() &&
            self.get_log(max_ready - 1)?.term == self.current_term()?
        {
            for i in self.commits_len()..max_ready {
                self.push_commit(&self.get_log(i)?.msg).await?;
            }
        }

        Ok(())
    }
}
