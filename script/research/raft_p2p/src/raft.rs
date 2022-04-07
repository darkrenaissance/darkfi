use async_std::{
    sync::{Arc, Mutex},
    task,
};
use std::{cmp::min, collections::HashMap, net::SocketAddr, path::PathBuf, time::Duration};

use async_executor::Executor;
use borsh::BorshSerialize;
use futures::{select, FutureExt};
use log::error;
use rand::Rng;

use darkfi::{net, Result};

use crate::{
    try_from_slice_unchecked, DataStore, Log, LogRequest, LogResponse, NetMsg, NetMsgMethod,
    NodeId, ProtocolRaft, Role, VecR, VoteRequest, VoteResponse,
};

const HEARTBEATTIMEOUT: u64 = 100;
const TIMEOUT: u64 = 300;

pub struct Raft {
    // this will be derived from the ip
    id: NodeId,

    // these five vars should be on local storage
    current_term: u64,
    voted_for: Option<NodeId>,
    logs: VecR<Log>,
    commit_length: u64,
    // the log will be added to this vector if it's committed by the majority of nodes
    commits: Arc<Mutex<Vec<Vec<u8>>>>,

    role: Role,

    current_leader: Option<NodeId>,

    votes_received: VecR<NodeId>,

    sent_length: HashMap<NodeId, u64>,
    acked_length: HashMap<NodeId, u64>,

    nodes: VecR<NodeId>,

    last_term: u64,

    send_queues: Option<async_channel::Sender<NetMsg>>,

    datastore: DataStore,
}

impl Raft {
    pub fn new(addr: SocketAddr, db_path: PathBuf) -> Result<Self> {
        if db_path.to_str().is_none() {
            error!(target: "raft", "datastore path is incorrect");
            return Err(darkfi::Error::ParseFailed("unable to parse pathbuf to str"))
        };

        let db_path_str = db_path.to_str().unwrap();

        let mut current_term = 0;
        let mut voted_for = None;
        let mut logs = VecR(vec![]);
        let mut commit_length = 0;
        let mut commits = Arc::new(Mutex::new(vec![]));

        let datastore = if db_path.exists() {
            let datastore = DataStore::new(db_path_str)?;
            current_term = datastore.current_term.get_last()?.unwrap_or(0);
            voted_for = datastore.voted_for.get_last()?.flatten();
            logs = VecR(datastore.logs.get_all()?);
            commit_length = datastore.commits_length.get_last()?.unwrap_or(0);
            commits = Arc::new(Mutex::new(datastore.commits.get_all()?));
            datastore
        } else {
            DataStore::new(db_path_str)?
        };

        Ok(Self {
            id: NodeId::from(addr),
            current_term,
            voted_for,
            logs,
            commit_length,
            commits,
            role: Role::Follower,
            current_leader: None,
            votes_received: VecR(vec![]),
            sent_length: HashMap::new(),
            acked_length: HashMap::new(),
            nodes: VecR(vec![]),
            last_term: 0,
            send_queues: None,
            datastore,
        })
    }

    pub async fn start(
        &mut self,
        net_settings: net::Settings,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        let (p2p_snd, receive_queues) = async_channel::unbounded::<NetMsg>();
        let (send_queues, p2p_recv) = async_channel::unbounded::<NetMsg>();

        self.send_queues = Some(send_queues);

        let p2p = net::P2p::new(net_settings).await;
        let p2p = p2p.clone();

        let registry = p2p.protocol_registry();

        let self_id = self.id.clone();
        registry
            .register(!net::SESSION_SEED, move |channel, p2p| {
                let self_id = self_id.clone();
                let sender = p2p_snd.clone();
                async move { ProtocolRaft::init(self_id, channel, sender, p2p).await }
            })
            .await;

        // P2p performs seed session
        p2p.clone().start(executor.clone()).await?;

        // TODO load the peers ips after the seed session finished
        // we can drive the nodes ids from the ips

        executor.spawn(p2p.clone().run(executor.clone())).detach();

        executor
            .spawn(async move {
                loop {
                    let msg: NetMsg = p2p_recv.recv().await.unwrap();
                    p2p.broadcast(msg).await.unwrap();
                }
            })
            .detach();

        let mut rng = rand::thread_rng();

        loop {
            let timeout = Duration::from_millis(rng.gen_range(0..200) + TIMEOUT);
            let heartbeat_timeout = Duration::from_millis(HEARTBEATTIMEOUT);

            if self.role == Role::Leader {
                select! {
                    m =  receive_queues.recv().fuse() => self.handle_method(m?).await?,
                    _ = task::sleep(heartbeat_timeout).fuse() => self.send_heartbeat().await,
                }
            } else {
                select! {
                    m =  receive_queues.recv().fuse() => self.handle_method(m?).await?,
                    _ = task::sleep(timeout).fuse() => self.send_vote_request().await,
                }
            }
        }
    }

    pub fn get_commits(&self) -> Arc<Mutex<Vec<Vec<u8>>>> {
        self.commits.clone()
    }

    pub async fn broadcast_msg(&mut self, msg: Vec<u8>) {
        if self.role == Role::Leader {
            let log = Log { msg, term: self.current_term };
            self.push_log(&log).unwrap();

            self.acked_length.insert(self.id.clone(), self.logs.len());

            let nodes = self.nodes.0.clone();
            for node in nodes.iter() {
                self.update_logs(node).await;
            }
        }
    }

    async fn handle_method(&mut self, msg: NetMsg) -> Result<()> {
        match msg.method {
            NetMsgMethod::LogResponse => {
                let lr: LogResponse = try_from_slice_unchecked(&msg.payload)?;
                self.receive_log_response(lr).await;
            }
            NetMsgMethod::LogRequest => {
                let lr: LogRequest = try_from_slice_unchecked(&msg.payload)?;
                self.receive_log_request(lr).await;
            }
            NetMsgMethod::VoteResponse => {
                let vr: VoteResponse = try_from_slice_unchecked(&msg.payload)?;
                self.receive_vote_response(vr).await;
            }
            NetMsgMethod::VoteRequest => {
                let vr: VoteRequest = try_from_slice_unchecked(&msg.payload)?;
                self.receive_vote_request(vr).await;
            }
        }
        Ok(())
    }
    async fn send(&self, recipient_id: Option<NodeId>, payload: &[u8], method: NetMsgMethod) {
        if let Some(sender) = &self.send_queues {
            let rnd = rand::random();
            let net_msg = NetMsg { id: rnd, recipient_id, payload: payload.to_vec(), method };
            sender.send(net_msg).await.unwrap();
        }
    }

    async fn send_heartbeat(&self) {
        if self.role == Role::Leader {
            let nodes = self.nodes.0.clone();
            for node in nodes.iter() {
                self.update_logs(node).await;
            }
        }
    }

    async fn send_vote_request(&mut self) {
        self.set_current_term(&(self.current_term + 1)).unwrap();
        self.role = Role::Candidate;
        self.set_voted_for(&Some(self.id.clone())).unwrap();
        self.votes_received.push(&self.id);

        self.reset_last_term();

        let request = VoteRequest {
            node_id: self.id.clone(),
            current_term: self.current_term,
            log_length: self.logs.len(),
            last_term: self.last_term,
        };

        let payload = request.try_to_vec().unwrap();
        self.send(None, &payload, NetMsgMethod::VoteRequest).await;
    }

    async fn receive_vote_request(&mut self, vr: VoteRequest) {
        if vr.current_term > self.current_term {
            self.set_current_term(&vr.current_term).unwrap();
            self.set_voted_for(&None).unwrap();
            self.role = Role::Follower;
        }

        self.reset_last_term();

        // check the logs of the candidate
        let vote_ok = (vr.last_term > self.last_term) ||
            (vr.last_term == self.last_term && vr.log_length >= self.logs.len());

        // slef.voted_for equal to vr.node_id or is None or voted to someone else
        let vote = if let Some(voted_for) = self.voted_for.as_ref() {
            *voted_for == vr.node_id
        } else {
            true
        };

        let mut response =
            VoteResponse { node_id: self.id.clone(), current_term: self.current_term, ok: false };

        if vr.current_term == self.current_term && vote_ok && vote {
            self.set_voted_for(&Some(vr.node_id.clone())).unwrap();
            response.set_ok(true);
        }

        let payload = response.try_to_vec().unwrap();
        self.send(Some(vr.node_id), &payload, NetMsgMethod::VoteResponse).await;
    }

    async fn receive_vote_response(&mut self, vr: VoteResponse) {
        if self.role == Role::Candidate && vr.current_term == self.current_term && vr.ok {
            self.votes_received.push(&vr.node_id);

            if self.votes_received.len() >= ((self.nodes.len() + 1) / 2) {
                self.role = Role::Leader;
                self.current_leader = Some(self.id.clone());
                for node in self.nodes.0.iter() {
                    self.sent_length.insert(node.clone(), self.logs.len());
                    self.acked_length.insert(node.clone(), 0);
                    self.update_logs(node).await;
                }
            }
        } else if vr.current_term > self.current_term {
            self.set_current_term(&vr.current_term).unwrap();
            self.role = Role::Follower;
            self.set_voted_for(&None).unwrap();
        }
    }

    async fn update_logs(&self, node_id: &NodeId) {
        let prefix_len = self.sent_length[node_id];
        let suffix: VecR<Log> = self.logs.slice_from(prefix_len);

        let mut prefix_term = 0;
        if prefix_len > 0 {
            prefix_term = self.logs.get(prefix_len - 1).term;
        }

        let request = LogRequest {
            leader_id: self.id.clone(),
            current_term: self.current_term,
            prefix_len,
            prefix_term,
            commit_length: self.commit_length,
            suffix,
        };

        let payload = request.try_to_vec().unwrap();
        self.send(Some(node_id.clone()), &payload, NetMsgMethod::LogRequest).await;
    }

    async fn receive_log_request(&mut self, lr: LogRequest) {
        if lr.current_term > self.current_term {
            self.set_current_term(&lr.current_term).unwrap();
            self.set_voted_for(&None).unwrap();
        }

        if lr.current_term == self.current_term {
            self.role = Role::Follower;
            self.current_leader = Some(lr.leader_id.clone());
        }

        let ok = (self.logs.len() >= lr.prefix_len) &&
            (lr.prefix_len == 0 || self.logs.get(lr.prefix_len - 1).term == lr.prefix_term);

        let response: LogResponse = if lr.current_term == self.current_term && ok {
            self.append_log(lr.prefix_len, lr.commit_length, &lr.suffix).await;
            let ack = lr.prefix_len + lr.suffix.len();
            LogResponse { node_id: self.id.clone(), current_term: self.current_term, ack, ok }
        } else {
            LogResponse {
                node_id: self.id.clone(),
                current_term: self.current_term,
                ack: 0,
                ok: false,
            }
        };

        let payload = response.try_to_vec().unwrap();
        self.send(Some(lr.leader_id.clone()), &payload, NetMsgMethod::LogResponse).await;
    }

    async fn receive_log_response(&mut self, lr: LogResponse) {
        if lr.current_term == self.current_term && self.role == Role::Leader {
            if lr.ok && lr.ack >= self.acked_length[&lr.node_id] {
                self.sent_length.insert(lr.node_id.clone(), lr.ack);
                self.acked_length.insert(lr.node_id, lr.ack);
                self.commit_log().await;
            } else if self.sent_length[&lr.node_id] > 0 {
                self.sent_length.insert(lr.node_id.clone(), self.sent_length[&lr.node_id] - 1);
                self.update_logs(&lr.node_id).await;
            }
        } else if lr.current_term > self.current_term {
            self.set_current_term(&lr.current_term).unwrap();
            self.role = Role::Follower;
            self.set_voted_for(&None).unwrap();
        }
    }

    fn reset_last_term(&mut self) {
        self.last_term = 0;

        if let Some(log) = self.logs.0.last() {
            self.last_term = log.term;
        }
    }

    fn acks(&self, length: u64) -> VecR<NodeId> {
        VecR::<NodeId>(
            self.nodes.0.clone().into_iter().filter(|n| self.acked_length[n] >= length).collect(),
        )
    }

    async fn commit_log(&mut self) {
        let min_acks = (self.nodes.len() + 1) / 2;

        let ready: Vec<u64> = self
            .logs
            .0
            .iter()
            .enumerate()
            .filter(|(i, _)| self.acks(*i as u64).len() >= min_acks)
            .map(|(i, _)| i as u64)
            .collect();

        if ready.is_empty() {
            return
        }

        let max_ready = *ready.iter().max().unwrap();
        if max_ready > self.commit_length && self.logs.get(max_ready - 1).term == self.current_term
        {
            for i in self.commit_length..(max_ready - 1) {
                self.push_commit(&self.logs.get(i).msg).await.unwrap();
            }

            self.set_commit_length(&max_ready).unwrap();
        }
    }

    async fn append_log(&mut self, prefix_len: u64, leader_commit: u64, suffix: &VecR<Log>) {
        if suffix.len() > 0 && self.logs.len() > prefix_len {
            let index = min(self.logs.len(), prefix_len + suffix.len()) - 1;
            if self.logs.get(index).term != suffix.get(index - prefix_len).term {
                self.push_logs(&self.logs.slice_to(prefix_len - 1)).unwrap();
            }
        }

        if prefix_len + suffix.len() > self.logs.len() {
            for i in (self.logs.len() - prefix_len)..(suffix.len() - 1) {
                self.push_log(&suffix.get(i)).unwrap();
            }
        }

        if leader_commit > self.commit_length {
            for i in self.commit_length..(leader_commit - 1) {
                self.push_commit(&self.logs.get(i).msg).await.unwrap();
            }
            self.set_commit_length(&leader_commit).unwrap();
        }
    }

    fn set_commit_length(&mut self, i: &u64) -> Result<()> {
        self.commit_length = *i;
        self.datastore.commits_length.insert(i)
    }
    fn set_current_term(&mut self, i: &u64) -> Result<()> {
        self.current_term = *i;
        self.datastore.current_term.insert(i)
    }
    fn set_voted_for(&mut self, i: &Option<NodeId>) -> Result<()> {
        self.voted_for = i.clone();
        self.datastore.voted_for.insert(i)
    }
    async fn push_commit(&mut self, i: &Vec<u8>) -> Result<()> {
        self.commits.lock().await.push(i.clone());
        self.datastore.commits.insert(i)
    }
    fn push_log(&mut self, i: &Log) -> Result<()> {
        self.logs.push(i);
        self.datastore.logs.insert(i)
    }
    fn push_logs(&mut self, i: &VecR<Log>) -> Result<()> {
        self.logs = i.clone();
        self.datastore.logs.wipe_insert_all(&i.to_vec())
    }
}
