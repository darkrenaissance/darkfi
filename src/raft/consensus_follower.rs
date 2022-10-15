use std::cmp::min;

use darkfi_serial::{serialize, Decodable, Encodable};
use log::debug;

use super::{
    primitives::{LogRequest, LogResponse, Logs, NetMsgMethod, Role, VoteRequest, VoteResponse},
    Raft,
};
use crate::Result;

impl<T: Decodable + Encodable + Clone> Raft<T> {
    pub(super) async fn receive_vote_request(&mut self, vr: VoteRequest) -> Result<()> {
        if vr.current_term > self.current_term()? {
            self.set_current_term(&vr.current_term)?;
            self.set_voted_for(&None)?;
            self.role = Role::Follower;
        }

        self.reset_last_term()?;

        // check the logs of the candidate
        let vote_ok = (vr.last_term > self.last_term) ||
            (vr.last_term == self.last_term && vr.log_length >= self.logs_len());

        // slef.voted_for equal to vr.node_id or is None or voted to someone else
        let vote =
            if let Some(voted_for) = self.voted_for()? { voted_for == vr.node_id } else { true };

        let mut response =
            VoteResponse { node_id: self.id(), current_term: self.current_term()?, ok: false };

        if vr.current_term == self.current_term()? && vote_ok && vote {
            self.set_voted_for(&Some(vr.node_id.clone()))?;
            response.set_ok(true);
        }

        let payload = serialize(&response);
        self.send(Some(vr.node_id), &payload, NetMsgMethod::VoteResponse, None).await
    }

    pub(super) async fn receive_log_request(&mut self, lr: LogRequest) -> Result<()> {
        debug!(target: "raft",
        "Receive LogRequest current_term: {} prefix_term: {} prefix_len: {} commit_length: {} suffixlen {}",
        lr.current_term, lr.prefix_term, lr.prefix_len, lr.commit_length, lr.suffix.len(),
        );

        if lr.current_term > self.current_term()? {
            self.set_current_term(&lr.current_term)?;
            self.set_voted_for(&None)?;
        }

        if lr.current_term == self.current_term()? {
            self.role = Role::Follower;
            self.current_leader = lr.leader_id.clone();
        }

        let mut ok = (self.logs_len() >= lr.prefix_len) &&
            (lr.prefix_len == 0 || self.get_log(lr.prefix_len - 1)?.term == lr.prefix_term);

        let mut ack = 0;

        if lr.current_term == self.current_term()? && ok {
            self.append_log(lr.prefix_len, lr.commit_length, &lr.suffix).await?;
            ack = lr.prefix_len + lr.suffix.len();
        } else {
            ok = false;
        }

        let response =
            LogResponse { node_id: self.id(), current_term: self.current_term()?, ack, ok };

        debug!(target: "raft",
         "Send LogResponse current_term: {} ack: {} ok: {}",
         response.current_term, response.ack, response.ok
        );

        let payload = serialize(&response);
        self.send(Some(lr.leader_id.clone()), &payload, NetMsgMethod::LogResponse, None).await
    }

    async fn append_log(
        &mut self,
        prefix_len: u64,
        leader_commit: u64,
        suffix: &Logs,
    ) -> Result<()> {
        if !suffix.is_empty() && self.logs_len() > prefix_len {
            let index = min(self.logs_len(), prefix_len + suffix.len()) - 1;
            if self.get_log(index)?.term != suffix.get(index - prefix_len)?.term {
                self.push_logs(&self.slice_logs_to(prefix_len)?)?;
            }
        }

        if prefix_len + suffix.len() > self.logs_len() {
            for i in (self.logs_len() - prefix_len)..suffix.len() {
                self.push_log(&suffix.get(i)?)?;
            }
        }

        if leader_commit > self.commits_len() {
            for i in self.commits_len()..leader_commit {
                self.push_commit(&self.get_log(i)?.msg).await?;
            }
        }

        Ok(())
    }
}
