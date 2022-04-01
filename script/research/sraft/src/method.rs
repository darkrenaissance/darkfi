use borsh::{BorshDeserialize, BorshSerialize};

use crate::LogEntry;

#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub enum RaftMethod {
    Vote(VoteArgs),
    Heartbeat(HeartbeatArgs),
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug)]
pub struct VoteArgs {
    pub term: u64,
    pub candidate_id: u64,
}

#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub struct VoteReply {
    pub term: u64,
    pub vote_granted: bool,
}

#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub struct HeartbeatArgs {
    pub term: u64,
    pub leader_id: u64,

    pub prev_log_index: u64,
    pub prev_log_term: u64,

    pub entries: Vec<LogEntry>,
    pub leader_commit: u64,
}

#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub struct HeartbeatReply {
    pub success: bool,
    pub term: u64,
    pub next_index: u64,
}
