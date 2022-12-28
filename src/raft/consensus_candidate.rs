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

use chrono::Utc;
use darkfi_serial::{serialize, Decodable, Encodable};
use log::info;

use crate::Result;

use super::{
    primitives::{NetMsgMethod, Role, VoteRequest, VoteResponse},
    Raft,
};

impl<T: Decodable + Encodable + Clone> Raft<T> {
    pub(super) async fn send_vote_request(&mut self) -> Result<()> {
        if self.role == Role::Leader {
            return Ok(())
        }

        let last_heartbeat_duration = Utc::now().timestamp() - self.last_heartbeat;

        if last_heartbeat_duration < self.settings.timeout as i64 {
            return Ok(())
        }

        self.set_current_term(&(self.current_term()? + 1))?;

        if self.role != Role::Candidate {
            info!(target: "raft", "Set the node role as Candidate");
            self.role = Role::Candidate;
        }

        self.set_voted_for(&Some(self.id()))?;
        self.votes_received = vec![self.id()];

        self.reset_last_term()?;

        let request = VoteRequest {
            node_id: self.id(),
            current_term: self.current_term()?,
            log_length: self.logs_len(),
            last_term: self.last_term,
        };

        let payload = serialize(&request);
        self.send(None, &payload, NetMsgMethod::VoteRequest, None).await
    }

    pub(super) async fn receive_vote_response(&mut self, vr: VoteResponse) -> Result<()> {
        if self.role == Role::Candidate && vr.current_term == self.current_term()? && vr.ok {
            if self.votes_received.contains(&vr.node_id) {
                return Ok(())
            }

            self.votes_received.push(vr.node_id);

            let nodes = self.nodes.lock().await;
            let nodes_cloned = nodes.clone();
            drop(nodes);

            if self.votes_received.len() >= ((nodes_cloned.len() + 1) / 2) {
                info!(target: "raft", "Set the node role as Leader");
                self.role = Role::Leader;
                self.current_leader = self.id();
                for node in nodes_cloned.iter() {
                    self.sent_length.insert(node.0, self.logs_len());
                    self.acked_length.insert(node.0, 0);
                }
            }
        } else if vr.current_term > self.current_term()? {
            self.set_current_term(&vr.current_term)?;
            self.role = Role::Follower;
            self.set_voted_for(&None)?;
        }

        Ok(())
    }
}
