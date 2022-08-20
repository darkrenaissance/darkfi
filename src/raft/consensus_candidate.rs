use log::info;

use crate::{
    util::serial::{serialize, Decodable, Encodable},
    Result,
};

use super::{
    primitives::{NetMsgMethod, Role, VoteRequest, VoteResponse},
    Raft,
};

impl<T: Decodable + Encodable + Clone> Raft<T> {
    pub(super) async fn send_vote_request(&mut self) -> Result<()> {
        let self_id = self.id();

        self.set_current_term(&(self.current_term()? + 1))?;

        if self.role != Role::Candidate {
            info!(target: "raft", "Set the node role as Candidate");
            self.role = Role::Candidate;
        }

        self.set_voted_for(&Some(self_id.clone()))?;
        self.votes_received = vec![];
        self.votes_received.push(self_id.clone());

        self.reset_last_term()?;

        let request = VoteRequest {
            node_id: self_id,
            current_term: self.current_term()?,
            log_length: self.logs_len(),
            last_term: self.last_term,
        };

        let payload = serialize(&request);
        self.send(None, &payload, NetMsgMethod::VoteRequest, None).await
    }

    pub(super) async fn receive_vote_response(&mut self, vr: VoteResponse) -> Result<()> {
        if self.role == Role::Candidate && vr.current_term == self.current_term()? && vr.ok {
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
