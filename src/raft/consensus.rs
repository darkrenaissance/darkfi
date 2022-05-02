use async_std::{
    sync::{Arc, Mutex},
    task,
};
use std::{cmp::min, collections::HashMap, net::SocketAddr, path::PathBuf, time::Duration};

use async_executor::Executor;
use futures::{select, FutureExt};
use log::{debug, error, info, warn};
use rand::{rngs::OsRng, Rng, RngCore};

use crate::{
    net,
    util::serial::{deserialize, serialize, Decodable, Encodable},
    Error, Result,
};

use super::{
    primitives::{
        Broadcast, BroadcastMsgRequest, Log, LogRequest, LogResponse, Logs, MapLength, NetMsg,
        NetMsgMethod, NodeId, Role, Sender, SyncRequest, SyncResponse, VoteRequest, VoteResponse,
    },
    DataStore,
};

const HEARTBEATTIMEOUT: u64 = 100;
const TIMEOUT: u64 = 300;
const TIMEOUT_NODES: u64 = 300;

async fn load_node_ids_loop(
    nodes: Arc<Mutex<HashMap<NodeId, SocketAddr>>>,
    p2p: net::P2pPtr,
    role: Role,
) -> Result<()> {
    if role == Role::Listener {
        return Ok(())
    }
    loop {
        debug!(target: "raft", "load node ids from p2p hosts ips");
        task::sleep(Duration::from_millis(TIMEOUT_NODES * 10)).await;
        let hosts = p2p.hosts().clone();
        let nodes_ip = hosts.load_all().await.clone();
        let mut nodes = nodes.lock().await;
        for ip in nodes_ip.iter() {
            nodes.insert(NodeId::from(*ip), *ip);
        }
        drop(nodes);
    }
}

async fn p2p_send_loop(receiver: async_channel::Receiver<NetMsg>, p2p: net::P2pPtr) -> Result<()> {
    loop {
        let msg: NetMsg = match receiver.recv().await {
            Ok(m) => m,
            Err(e) => {
                error!(target: "raft", "error occurred while receiving a msg: {}", e);
                continue
            }
        };
        match p2p.broadcast(msg).await {
            Ok(_) => {}
            Err(e) => {
                error!(target: "raft", "error occurred during broadcasting a msg: {}", e);
                continue
            }
        }
    }
}

pub struct Raft<T> {
    // this will be derived from the ip
    // if the node doesn't have an id then will become a listener and doesn't have the right
    // to request/response votes or response a confirmation for log
    pub id: Option<NodeId>,

    // these five vars should be on local storage
    current_term: u64,
    voted_for: Option<NodeId>,
    logs: Logs,
    commit_length: u64,

    role: Role,

    current_leader: Option<NodeId>,

    votes_received: Vec<NodeId>,

    sent_length: MapLength,
    acked_length: MapLength,

    nodes: Arc<Mutex<HashMap<NodeId, SocketAddr>>>,

    last_term: u64,

    sender: Sender,

    broadcast_msg: Broadcast<T>,
    broadcast_commits: Broadcast<T>,

    datastore: DataStore<T>,
}

impl<T: Decodable + Encodable + Clone> Raft<T> {
    pub fn new(addr: Option<SocketAddr>, db_path: PathBuf) -> Result<Self> {
        if db_path.to_str().is_none() {
            error!(target: "raft", "datastore path is incorrect");
            return Err(Error::ParseFailed("unable to parse pathbuf to str"))
        };

        let db_path_str = db_path.to_str().unwrap();

        let mut current_term = 0;
        let mut voted_for = None;
        let mut logs = Logs(vec![]);
        let mut commit_length = 0;

        let datastore = if db_path.exists() {
            let datastore = DataStore::new(db_path_str)?;
            current_term = datastore.current_term.get_last()?.unwrap_or(0);
            voted_for = datastore.voted_for.get_last()?.flatten();
            logs = Logs(datastore.logs.get_all()?);
            commit_length = datastore.commits_length.get_last()?.unwrap_or(0);
            datastore
        } else {
            DataStore::new(db_path_str)?
        };

        // broadcasting channels
        let broadcast_msg = async_channel::unbounded::<T>();
        let broadcast_commits = async_channel::unbounded::<T>();

        let sender = async_channel::unbounded::<NetMsg>();

        let id = addr.map(NodeId::from);
        let role = if id.is_some() { Role::Follower } else { Role::Listener };

        Ok(Self {
            id,
            current_term,
            voted_for,
            logs,
            commit_length,
            role,
            current_leader: None,
            votes_received: vec![],
            sent_length: MapLength(HashMap::new()),
            acked_length: MapLength(HashMap::new()),
            nodes: Arc::new(Mutex::new(HashMap::new())),
            last_term: 0,
            sender,
            broadcast_msg,
            broadcast_commits,
            datastore,
        })
    }

    pub async fn start(
        &mut self,
        p2p: net::P2pPtr,
        p2p_recv_channel: async_channel::Receiver<NetMsg>,
        executor: Arc<Executor<'_>>,
        stop_signal: async_channel::Receiver<()>,
    ) -> Result<()> {
        let receiver = self.sender.1.clone();
        let p2p_send_task = executor.spawn(p2p_send_loop(receiver.clone(), p2p.clone()));

        let load_ips_task =
            executor.spawn(load_node_ids_loop(self.nodes.clone(), p2p.clone(), self.role.clone()));

        // Sync listener node
        if self.role == Role::Listener {
            let last_term =
                if !self.logs.0.is_empty() { self.logs.0.last().unwrap().term } else { 0 };

            let sync_request = SyncRequest { logs_len: self.logs.len(), last_term };

            info!("send sync request");
            self.send(None, &serialize(&sync_request), NetMsgMethod::SyncRequest, None).await?;

            self.waiting_for_sync(p2p_recv_channel.clone(), stop_signal.clone()).await?;
        }

        let mut rng = rand::thread_rng();

        let broadcast_msg_rv = self.broadcast_msg.1.clone();

        loop {
            let timeout: Duration = if self.role == Role::Leader {
                Duration::from_millis(HEARTBEATTIMEOUT)
            } else {
                Duration::from_millis(rng.gen_range(0..200) + TIMEOUT)
            };

            let result: Result<()>;

            select! {
                m =  p2p_recv_channel.recv().fuse() => result = self.handle_method(m?).await,
                m =  broadcast_msg_rv.recv().fuse() => result = self.broadcast_msg(&m?,None).await,
                _ = task::sleep(timeout).fuse() => {
                    result = if self.role == Role::Leader {
                        self.send_heartbeat().await
                    }else {
                        self.send_vote_request().await
                    };
                },
                _ = stop_signal.recv().fuse() => break,
            }

            match result {
                Ok(_) => {}
                Err(e) => warn!(target: "raft", "warn: {}", e),
            }
        }

        warn!(target: "raft", "Raft start() Exit Signal");
        load_ips_task.cancel().await;
        p2p_send_task.cancel().await;
        self.datastore.cancel().await?;
        Ok(())
    }

    pub fn get_commits(&self) -> async_channel::Receiver<T> {
        self.broadcast_commits.1.clone()
    }

    pub fn get_broadcast(&self) -> async_channel::Sender<T> {
        self.broadcast_msg.0.clone()
    }

    async fn broadcast_msg(&mut self, msg: &T, msg_id: Option<u64>) -> Result<()> {
        if self.role == Role::Leader {
            let msg = serialize(msg);
            let log = Log { msg, term: self.current_term };
            self.push_log(&log)?;

            self.acked_length.insert(&self.id.clone().unwrap(), self.logs.len());
        } else {
            let b_msg = BroadcastMsgRequest(serialize(msg));
            self.send(
                self.current_leader.clone(),
                &serialize(&b_msg),
                NetMsgMethod::BroadcastRequest,
                msg_id,
            )
            .await?;
        }

        info!(target: "raft", "Role: {:?}, broadcast a msg id: {:?} ", self.role, msg_id);

        Ok(())
    }

    async fn handle_method(&mut self, msg: NetMsg) -> Result<()> {
        match msg.method {
            NetMsgMethod::LogResponse => {
                let lr: LogResponse = deserialize(&msg.payload)?;
                self.receive_log_response(lr).await?;
            }
            NetMsgMethod::LogRequest => {
                let lr: LogRequest = deserialize(&msg.payload)?;
                self.receive_log_request(lr).await?;
            }
            NetMsgMethod::VoteResponse => {
                let vr: VoteResponse = deserialize(&msg.payload)?;
                self.receive_vote_response(vr).await?;
            }
            NetMsgMethod::VoteRequest => {
                let vr: VoteRequest = deserialize(&msg.payload)?;
                self.receive_vote_request(vr).await?;
            }
            NetMsgMethod::BroadcastRequest => {
                let vr: BroadcastMsgRequest = deserialize(&msg.payload)?;
                let d: T = deserialize(&vr.0)?;
                self.broadcast_msg(&d, Some(msg.id)).await?;
            }
            NetMsgMethod::SyncRequest => {
                info!("receive sync request");
                let sr: SyncRequest = deserialize(&msg.payload)?;
                self.receive_sync_request(&sr, msg.id).await?;
            }
            NetMsgMethod::SyncResponse => {}
        }

        debug!(target: "raft", "Role: {:?}  receive msg id: {}  recipient_id: {:?} method: {:?} ",
        self.role, msg.id, &msg.recipient_id.is_some(), &msg.method);
        Ok(())
    }

    async fn receive_sync_request(&self, sr: &SyncRequest, msg_id: u64) -> Result<()> {
        if self.role == Role::Leader {
            let mut wipe = false;

            let logs = if sr.logs_len == 0 {
                self.logs.clone()
            } else if self.logs.len() >= sr.logs_len &&
                self.logs.get(sr.logs_len - 1)?.term == sr.last_term
            {
                self.logs.slice_from(sr.logs_len).unwrap()
            } else {
                wipe = true;
                self.logs.clone()
            };

            let sync_response = SyncResponse {
                logs,
                commit_length: self.commit_length,
                leader_id: self.id.clone().unwrap(),
                wipe,
            };

            info!("send sync response");
            for _ in 0..2 {
                self.send(
                    self.current_leader.clone(),
                    &serialize(&sync_response),
                    NetMsgMethod::SyncResponse,
                    None,
                )
                .await?;
            }
        } else {
            self.send(
                self.current_leader.clone(),
                &serialize(sr),
                NetMsgMethod::SyncRequest,
                Some(msg_id),
            )
            .await?;
        }

        Ok(())
    }

    async fn receive_sync_response(&mut self, sr: &SyncResponse) -> Result<()> {
        info!("receive sync response");
        if sr.wipe {
            self.set_commit_length(&0)?;
            self.push_logs(&sr.logs)?;
        } else {
            for log in sr.logs.0.iter() {
                self.push_log(log)?;
            }
        }

        if !self.logs.is_empty() {
            self.set_current_term(&self.logs.0.last().unwrap().term.clone())?;
        }

        for i in self.commit_length..sr.commit_length {
            self.push_commit(&self.logs.get(i)?.msg).await?;
        }

        self.set_commit_length(&sr.commit_length)?;

        self.current_leader = Some(sr.leader_id.clone());

        Ok(())
    }

    async fn send(
        &self,
        recipient_id: Option<NodeId>,
        payload: &[u8],
        method: NetMsgMethod,
        msg_id: Option<u64>,
    ) -> Result<()> {
        let random_id = if msg_id.is_some() { msg_id.unwrap() } else { OsRng.next_u64() };

        debug!(target: "raft","Role: {:?}  send a msg id: {}  recipient_id: {:?} method: {:?} ",
        self.role, random_id, &recipient_id.is_some(), &method);

        let net_msg = NetMsg { id: random_id, recipient_id, payload: payload.to_vec(), method };
        self.sender.0.send(net_msg).await?;

        Ok(())
    }

    async fn waiting_for_sync(
        &mut self,
        p2p_recv_channel: async_channel::Receiver<NetMsg>,
        stop_signal: async_channel::Receiver<()>,
    ) -> Result<()> {
        loop {
            select! {
                msg =  p2p_recv_channel.recv().fuse() => {
                    let msg = msg?;
                    if msg.method == NetMsgMethod::SyncResponse {
                        let sr: SyncResponse = deserialize(&msg.payload)?;
                        self.receive_sync_response(&sr).await?;
                        break
                    }},
                    _ = stop_signal.recv().fuse() => break,
            }
        }
        Ok(())
    }

    async fn send_heartbeat(&self) -> Result<()> {
        if self.role == Role::Leader {
            let nodes = self.nodes.lock().await;
            let nodes_cloned = nodes.clone();
            drop(nodes);
            for node in nodes_cloned.iter() {
                self.update_logs(node.0).await?;
            }
        }
        Ok(())
    }

    async fn send_vote_request(&mut self) -> Result<()> {
        if self.role == Role::Listener {
            return Ok(())
        }

        let self_id = self.id.clone().unwrap();

        self.set_current_term(&(self.current_term + 1))?;
        self.role = Role::Candidate;
        self.set_voted_for(&Some(self_id.clone()))?;
        self.votes_received.push(self_id.clone());

        self.reset_last_term();

        let request = VoteRequest {
            node_id: self_id,
            current_term: self.current_term,
            log_length: self.logs.len(),
            last_term: self.last_term,
        };

        let payload = serialize(&request);
        self.send(None, &payload, NetMsgMethod::VoteRequest, None).await
    }

    async fn receive_vote_request(&mut self, vr: VoteRequest) -> Result<()> {
        if self.role == Role::Listener {
            return Ok(())
        }

        if vr.current_term > self.current_term {
            self.set_current_term(&vr.current_term)?;
            self.set_voted_for(&None)?;
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

        let mut response = VoteResponse {
            node_id: self.id.clone().unwrap(),
            current_term: self.current_term,
            ok: false,
        };

        if vr.current_term == self.current_term && vote_ok && vote {
            self.set_voted_for(&Some(vr.node_id.clone()))?;
            response.set_ok(true);
        }

        let payload = serialize(&response);
        self.send(Some(vr.node_id), &payload, NetMsgMethod::VoteResponse, None).await
    }

    async fn receive_vote_response(&mut self, vr: VoteResponse) -> Result<()> {
        if self.role == Role::Listener {
            return Ok(())
        }

        if self.role == Role::Candidate && vr.current_term == self.current_term && vr.ok {
            self.votes_received.push(vr.node_id);

            let nodes = self.nodes.lock().await;
            let nodes_cloned = nodes.clone();
            drop(nodes);

            if self.votes_received.len() >= ((nodes_cloned.len() + 1) / 2) {
                self.role = Role::Leader;
                self.current_leader = Some(self.id.clone().unwrap());
                for node in nodes_cloned.iter() {
                    self.sent_length.insert(node.0, self.logs.len());
                    self.acked_length.insert(node.0, 0);
                }
            }
        } else if vr.current_term > self.current_term {
            self.set_current_term(&vr.current_term)?;
            self.role = Role::Follower;
            self.set_voted_for(&None)?;
        }

        Ok(())
    }

    async fn update_logs(&self, node_id: &NodeId) -> Result<()> {
        let prefix_len = match self.sent_length.get(node_id) {
            Ok(len) => len,
            Err(_) => {
                // return if failed to index
                return Ok(())
            }
        };

        let suffix: Logs = if self.logs.slice_from(prefix_len).is_some() {
            self.logs.slice_from(prefix_len).unwrap()
        } else {
            return Ok(())
        };

        let mut prefix_term = 0;

        if prefix_len > 0 {
            prefix_term = self.logs.get(prefix_len - 1)?.term;
        }

        let request = LogRequest {
            leader_id: self.id.clone().unwrap(),
            current_term: self.current_term,
            prefix_len,
            prefix_term,
            commit_length: self.commit_length,
            suffix,
        };

        let payload = serialize(&request);
        self.send(Some(node_id.clone()), &payload, NetMsgMethod::LogRequest, None).await
    }

    async fn receive_log_request(&mut self, lr: LogRequest) -> Result<()> {
        if lr.current_term > self.current_term {
            self.set_current_term(&lr.current_term)?;
            self.set_voted_for(&None)?;
        }

        if lr.current_term == self.current_term {
            if self.role != Role::Listener {
                self.role = Role::Follower;
            }
            self.current_leader = Some(lr.leader_id.clone());
        }

        let mut ok = (self.logs.len() >= lr.prefix_len) &&
            (lr.prefix_len == 0 || self.logs.get(lr.prefix_len - 1)?.term == lr.prefix_term);

        let mut ack = 0;

        if lr.current_term == self.current_term && ok {
            self.append_log(lr.prefix_len, lr.commit_length, &lr.suffix).await?;
            ack = lr.prefix_len + lr.suffix.len();
        } else {
            ok = false;
        }

        if self.role == Role::Listener {
            return Ok(())
        }

        let response = LogResponse {
            node_id: self.id.clone().unwrap(),
            current_term: self.current_term,
            ack,
            ok,
        };

        let payload = serialize(&response);
        self.send(Some(lr.leader_id.clone()), &payload, NetMsgMethod::LogResponse, None).await
    }

    async fn receive_log_response(&mut self, lr: LogResponse) -> Result<()> {
        if lr.current_term == self.current_term && self.role == Role::Leader {
            if lr.ok && lr.ack >= self.acked_length.get(&lr.node_id)? {
                self.sent_length.insert(&lr.node_id, lr.ack);
                self.acked_length.insert(&lr.node_id, lr.ack);
                self.commit_log().await?;
            } else if self.sent_length.get(&lr.node_id)? > 0 {
                self.sent_length.insert(&lr.node_id, self.sent_length.get(&lr.node_id)? - 1);
            }
        } else if lr.current_term > self.current_term {
            self.set_current_term(&lr.current_term)?;
            if self.role != Role::Listener {
                self.role = Role::Follower;
            }
            self.set_voted_for(&None)?;
        }

        Ok(())
    }

    fn reset_last_term(&mut self) {
        self.last_term = 0;

        if let Some(log) = self.logs.0.last() {
            self.last_term = log.term;
        }
    }

    fn acks(&self, nodes: HashMap<NodeId, SocketAddr>, length: u64) -> HashMap<NodeId, SocketAddr> {
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

        for len in 1..(self.logs.len() + 1) {
            if self.acks(nodes.clone(), len).len() >= min_acks {
                ready.push(len);
            }
        }

        if ready.is_empty() {
            return Ok(())
        }

        let max_ready = *ready.iter().max().unwrap();

        if max_ready > self.commit_length && self.logs.get(max_ready - 1)?.term == self.current_term
        {
            for i in self.commit_length..max_ready {
                self.push_commit(&self.logs.get(i)?.msg).await?;
            }

            self.set_commit_length(&max_ready)?;
        }

        Ok(())
    }

    async fn append_log(
        &mut self,
        prefix_len: u64,
        leader_commit: u64,
        suffix: &Logs,
    ) -> Result<()> {
        if !suffix.is_empty() && self.logs.len() > prefix_len {
            let index = min(self.logs.len(), prefix_len + suffix.len()) - 1;
            if self.logs.get(index)?.term != suffix.get(index - prefix_len)?.term {
                self.push_logs(&self.logs.slice_to(prefix_len))?;
            }
        }

        if prefix_len + suffix.len() > self.logs.len() {
            for i in (self.logs.len() - prefix_len)..suffix.len() {
                self.push_log(&suffix.get(i)?)?;
            }
        }

        if leader_commit > self.commit_length {
            for i in self.commit_length..leader_commit {
                self.push_commit(&self.logs.get(i)?.msg).await?;
            }
            self.set_commit_length(&leader_commit)?;
        }

        Ok(())
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
    async fn push_commit(&mut self, commit: &[u8]) -> Result<()> {
        let commit: T = deserialize(commit)?;
        self.broadcast_commits.0.send(commit.clone()).await?;
        self.datastore.commits.insert(&commit)
    }
    fn push_log(&mut self, i: &Log) -> Result<()> {
        self.logs.push(i);
        self.datastore.logs.insert(i)
    }
    fn push_logs(&mut self, i: &Logs) -> Result<()> {
        self.logs = i.clone();
        self.datastore.logs.wipe_insert_all(&i.to_vec())
    }
}
