use async_std::{
    sync::{Arc, Mutex},
    task,
};
use std::{cmp::min, path::PathBuf, time::Duration};

use async_executor::Executor;
use chrono::Utc;
use futures::{select, FutureExt};
use fxhash::FxHashMap;
use log::{debug, error, info, warn};
use rand::{rngs::OsRng, Rng, RngCore};
use url::Url;

use crate::{
    net,
    util::serial::{deserialize, serialize, Decodable, Encodable},
    Error, Result,
};

use super::{
    primitives::{
        BroadcastMsgRequest, Channel, Log, LogRequest, LogResponse, Logs, MapLength, NetMsg,
        NetMsgMethod, NodeId, Role, Sender, SyncRequest, SyncResponse, VoteRequest, VoteResponse,
    },
    DataStore,
};

// Milliseconds
const HEARTBEATTIMEOUT: u64 = 500;
const TIMEOUT: u64 = 6000;
const TIMEOUT_NODES: u64 = 1000;
const SYNC_TIMEOUT_FOR_EACH_ATTEMPT: u64 = 2000;

// Seconds
const SEEN_DURATION: i64 = 120;

const SYNC_ATTEMPTS: u64 = 30;

async fn load_node_ids_loop(
    nodes: Arc<Mutex<FxHashMap<NodeId, Url>>>,
    p2p: net::P2pPtr,
    role: Role,
) -> Result<()> {
    if role == Role::Listener {
        return Ok(())
    }

    let self_ip = p2p.settings().external_addr.as_ref().unwrap().clone();
    loop {
        debug!(target: "raft", "Loading node ids from p2p hosts",);
        task::sleep(Duration::from_millis(TIMEOUT_NODES)).await;
        let hosts = p2p.hosts().clone();
        let nodes_ip = hosts.load_all().await.clone();

        for ip in nodes_ip.iter() {
            if ip == &self_ip {
                continue
            }
            (*nodes.lock().await).insert(NodeId::from(ip.clone()), ip.clone());
        }
    }
}

// Auxilary function to periodically prun seen messages, based on when they were received.
// This helps us to prevent broadcasting loops.
async fn prune_seen_messages(map: Arc<Mutex<fxhash::FxHashMap<String, i64>>>) {
    loop {
        crate::util::sleep(SEEN_DURATION as u64).await;
        debug!("Pruning seen messages");

        let now = Utc::now().timestamp();

        let mut map = map.lock().await;
        for (k, v) in map.clone().iter() {
            if now - v > SEEN_DURATION {
                map.remove(k);
            }
        }
    }
}

async fn p2p_send_loop(receiver: async_channel::Receiver<NetMsg>, p2p: net::P2pPtr) -> Result<()> {
    loop {
        let msg: NetMsg = receiver.recv().await?;
        if let Err(e) = p2p.broadcast(msg).await {
            error!(target: "raft", "error occurred during broadcasting a msg: {}", e);
            continue
        }
    }
}

pub struct Raft<T> {
    // this will be derived from the ip
    pub id: Option<NodeId>,

    role: Role,

    current_leader: Option<NodeId>,

    votes_received: Vec<NodeId>,

    sent_length: MapLength,
    acked_length: MapLength,

    nodes: Arc<Mutex<FxHashMap<NodeId, Url>>>,

    last_term: u64,

    sender: Sender,

    msgs_channel: Channel<T>,
    commits_channel: Channel<T>,

    datastore: DataStore<T>,

    seen_msgs: Arc<Mutex<FxHashMap<String, i64>>>,
}

impl<T: Decodable + Encodable + Clone> Raft<T> {
    pub fn new(
        addr: Option<Url>,
        db_path: PathBuf,
        seen_msgs: Arc<Mutex<FxHashMap<String, i64>>>,
    ) -> Result<Self> {
        if db_path.to_str().is_none() {
            error!(target: "raft", "datastore path is incorrect");
            return Err(Error::ParseFailed("unable to parse pathbuf to str"))
        };

        let datastore = DataStore::new(db_path.to_str().unwrap())?;

        // broadcasting channels
        let msgs_channel = async_channel::unbounded::<T>();
        let commits_channel = async_channel::unbounded::<T>();

        let sender = async_channel::unbounded::<NetMsg>();

        let id = addr.map(NodeId::from);
        let role = if id.is_some() { Role::Follower } else { Role::Listener };

        Ok(Self {
            id,
            role,
            current_leader: None,
            votes_received: vec![],
            sent_length: MapLength(FxHashMap::default()),
            acked_length: MapLength(FxHashMap::default()),
            nodes: Arc::new(Mutex::new(FxHashMap::default())),
            last_term: 0,
            sender,
            msgs_channel,
            commits_channel,
            datastore,
            seen_msgs,
        })
    }

    pub async fn start(
        &mut self,
        p2p: net::P2pPtr,
        p2p_recv_channel: async_channel::Receiver<NetMsg>,
        executor: Arc<Executor<'_>>,
        stop_signal: async_channel::Receiver<()>,
    ) -> Result<()> {
        let p2p_send_task = executor.spawn(p2p_send_loop(self.sender.1.clone(), p2p.clone()));

        if self.role != Role::Listener {
            executor
                .spawn(load_node_ids_loop(self.nodes.clone(), p2p.clone(), self.role.clone()))
                .detach();
        }

        let prune_seen_messages_task = executor.spawn(prune_seen_messages(self.seen_msgs.clone()));

        let mut synced = true;

        // Sync listener node
        if self.role == Role::Listener {
            synced = false;
            let last_term = if !self.is_logs_empty() { self.last_log()?.unwrap().term } else { 0 };

            let sync_request_id = OsRng.next_u64();
            let sync_request =
                SyncRequest { id: sync_request_id, logs_len: self.logs_len(), last_term };

            info!("Start Syncing...");
            for _ in 0..SYNC_ATTEMPTS {
                if synced {
                    break
                }

                self.send(None, &serialize(&sync_request), NetMsgMethod::SyncRequest, None).await?;

                synced = self
                    .waiting_for_sync(
                        executor.clone(),
                        p2p_recv_channel.clone(),
                        stop_signal.clone(),
                        sync_request_id,
                    )
                    .await?;
            }
            if synced {
                info!("SYNCED SUCCESSFULLY!!");
            }
        }

        let mut rng = rand::thread_rng();

        let broadcast_msg_rv = self.msgs_channel.1.clone();

        if !synced {
            error!("SYNCING FAILED!!");
        } else {
            loop {
                let timeout: Duration = if self.role == Role::Leader {
                    Duration::from_millis(HEARTBEATTIMEOUT)
                } else {
                    Duration::from_millis(rng.gen_range(0..HEARTBEATTIMEOUT) + TIMEOUT)
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
        }

        warn!(target: "raft", "Raft Terminating...");
        p2p_send_task.cancel().await;
        prune_seen_messages_task.cancel().await;
        self.datastore.flush().await?;
        Ok(())
    }

    pub fn get_commits_channel(&self) -> async_channel::Receiver<T> {
        self.commits_channel.1.clone()
    }

    pub fn get_msgs_channel(&self) -> async_channel::Sender<T> {
        self.msgs_channel.0.clone()
    }

    async fn broadcast_msg(&mut self, msg: &T, msg_id: Option<u64>) -> Result<()> {
        if self.role == Role::Leader {
            let msg = serialize(msg);
            let log = Log { msg, term: self.current_term()? };
            self.push_log(&log)?;

            self.acked_length.insert(&self.id.clone().unwrap(), self.logs_len());
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

        debug!(target: "raft", "Role: {:?}, broadcast a msg id: {:?} ", self.role, msg_id);

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
                debug!(target: "raft", "Receive sync request");
                let sr: SyncRequest = deserialize(&msg.payload)?;
                self.receive_sync_request(&sr, msg.id).await?;
            }
            NetMsgMethod::SyncResponse => {}
        }

        debug!(target: "raft", "Role: {:?}  receive a msg with id: {}  recipient_id: {:?} method: {:?} ",
           self.role, msg.id, &msg.recipient_id.is_some(), &msg.method);
        Ok(())
    }

    async fn receive_sync_request(&self, sr: &SyncRequest, msg_id: u64) -> Result<()> {
        if self.role == Role::Leader {
            let mut wipe = false;

            let logs = if sr.logs_len == 0 {
                self.logs()?.clone()
            } else if self.logs_len() >= sr.logs_len &&
                self.get_log(sr.logs_len - 1)?.term == sr.last_term
            {
                self.slice_logs_from(sr.logs_len)?.unwrap()
            } else {
                wipe = true;
                self.logs()?.clone()
            };

            let sync_response = SyncResponse {
                id: sr.id,
                logs,
                commit_length: self.commits_len(),
                leader_id: self.id.clone().unwrap(),
                wipe,
            };

            debug!(target: "raft", "Send sync response");
            self.send(None, &serialize(&sync_response), NetMsgMethod::SyncResponse, None).await?;
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
        debug!(target: "raft", "Receive sync response");
        if sr.wipe {
            self.push_logs(&sr.logs)?;
        } else {
            for log in sr.logs.0.iter() {
                self.push_log(log)?;
            }
        }

        if !self.logs()?.is_empty() {
            self.set_current_term(&self.logs()?.0.last().unwrap().term.clone())?;
        }

        for i in self.commits_len()..sr.commit_length {
            self.push_commit(&self.get_log(i)?.msg).await?;
        }

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

        debug!(target: "raft","Role: {:?}  send a msg with id: {}  recipient_id: {:?} method: {:?} ",
           self.role, random_id, &recipient_id.is_some(), &method);

        let net_msg = NetMsg { id: random_id, recipient_id, payload: payload.to_vec(), method };
        self.seen_msgs.lock().await.insert(random_id.to_string(), Utc::now().timestamp());
        self.sender.0.send(net_msg).await?;

        Ok(())
    }

    async fn waiting_for_sync(
        &mut self,
        executor: Arc<Executor<'_>>,
        p2p_recv_channel: async_channel::Receiver<NetMsg>,
        stop_signal: async_channel::Receiver<()>,
        sync_request_id: u64,
    ) -> Result<bool> {
        let (timeout_s, timeout_r) = async_channel::unbounded::<()>();
        executor
            .spawn(async move {
                task::sleep(Duration::from_millis(SYNC_TIMEOUT_FOR_EACH_ATTEMPT)).await;
                timeout_s.send(()).await.unwrap_or(());
            })
            .detach();

        loop {
            select! {
                msg =  p2p_recv_channel.recv().fuse() => {
                        let msg = msg?;
                        if msg.method != NetMsgMethod::SyncResponse {
                            continue
                        }

                        let sr: SyncResponse = deserialize(&msg.payload)?;
                        if sr.id != sync_request_id {
                            continue
                        }

                        self.receive_sync_response(&sr).await?;
                        return Ok(true)
                    },
                _ = stop_signal.recv().fuse() => break,
                _ = timeout_r.recv().fuse() => break,
            }
        }
        Ok(false)
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

        self.set_current_term(&(self.current_term()? + 1))?;
        self.role = Role::Candidate;
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

    async fn receive_vote_request(&mut self, vr: VoteRequest) -> Result<()> {
        if self.role == Role::Listener {
            return Ok(())
        }

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

        let mut response = VoteResponse {
            node_id: self.id.clone().unwrap(),
            current_term: self.current_term()?,
            ok: false,
        };

        if vr.current_term == self.current_term()? && vote_ok && vote {
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

        if self.role == Role::Candidate && vr.current_term == self.current_term()? && vr.ok {
            self.votes_received.push(vr.node_id);

            let nodes = self.nodes.lock().await;
            let nodes_cloned = nodes.clone();
            drop(nodes);

            if self.votes_received.len() >= (nodes_cloned.len() / 2) {
                self.role = Role::Leader;
                self.current_leader = Some(self.id.clone().unwrap());
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

    // only the leader broadcast this
    async fn update_logs(&self, node_id: &NodeId) -> Result<()> {
        let prefix_len = match self.sent_length.get(node_id) {
            Ok(len) => len,
            Err(_) => {
                // return if failed to index
                return Ok(())
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
            leader_id: self.id.clone().unwrap(),
            current_term: self.current_term()?,
            prefix_len,
            prefix_term,
            commit_length: self.commits_len(),
            suffix,
        };

        let payload = serialize(&request);
        self.send(Some(node_id.clone()), &payload, NetMsgMethod::LogRequest, None).await
    }

    async fn receive_log_request(&mut self, lr: LogRequest) -> Result<()> {
        debug!(target: "raft",
            "Receive LogRequest current_term: {} prefix_term: {} prefix_len: {} commit_length: {} suffixlen {}",
            lr.current_term, lr.prefix_term, lr.prefix_len, lr.commit_length, lr.suffix.len(),
        );

        if lr.current_term > self.current_term()? {
            self.set_current_term(&lr.current_term)?;
            self.set_voted_for(&None)?;
        }

        if lr.current_term == self.current_term()? {
            if self.role != Role::Listener {
                self.role = Role::Follower;
            }
            self.current_leader = Some(lr.leader_id.clone());
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

        if self.role == Role::Listener {
            return Ok(())
        }

        let response = LogResponse {
            node_id: self.id.clone().unwrap(),
            current_term: self.current_term()?,
            ack,
            ok,
        };

        debug!(target: "raft",
            "Send LogResponse current_term: {} ack: {} ok: {}",
            response.current_term, response.ack, response.ok
        );

        let payload = serialize(&response);
        self.send(Some(lr.leader_id.clone()), &payload, NetMsgMethod::LogResponse, None).await
    }

    async fn receive_log_response(&mut self, lr: LogResponse) -> Result<()> {
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
            if self.role != Role::Listener {
                self.role = Role::Follower;
            }
            self.set_voted_for(&None)?;
        }

        Ok(())
    }

    fn reset_last_term(&mut self) -> Result<()> {
        self.last_term = 0;

        if let Some(log) = self.last_log()? {
            self.last_term = log.term;
        }

        Ok(())
    }

    fn acks(&self, nodes: FxHashMap<NodeId, Url>, length: u64) -> FxHashMap<NodeId, Url> {
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

    fn set_current_term(&mut self, i: &u64) -> Result<()> {
        self.datastore.current_term.insert(i)
    }
    fn set_voted_for(&mut self, i: &Option<NodeId>) -> Result<()> {
        self.datastore.voted_for.insert(i)
    }
    async fn push_commit(&mut self, commit: &[u8]) -> Result<()> {
        let commit: T = deserialize(commit)?;
        self.commits_channel.0.send(commit.clone()).await?;
        self.datastore.commits.insert(&commit)
    }
    fn push_log(&mut self, log: &Log) -> Result<()> {
        self.datastore.logs.insert(log)
    }
    fn push_logs(&mut self, logs: &Logs) -> Result<()> {
        self.datastore.logs.wipe_insert_all(&logs.to_vec())
    }
    fn is_logs_empty(&self) -> bool {
        self.datastore.logs.is_empty()
    }

    fn current_term(&self) -> Result<u64> {
        Ok(self.datastore.current_term.get_last()?.unwrap_or(0))
    }
    fn voted_for(&self) -> Result<Option<NodeId>> {
        Ok(self.datastore.voted_for.get_last()?.flatten())
    }
    fn commits_len(&self) -> u64 {
        self.datastore.commits.len()
    }
    fn logs(&self) -> Result<Logs> {
        Ok(Logs(self.datastore.logs.get_all()?))
    }
    fn logs_len(&self) -> u64 {
        self.datastore.logs.len()
    }
    fn last_log(&self) -> Result<Option<Log>> {
        self.datastore.logs.get_last()
    }
    fn get_log(&self, index: u64) -> Result<Log> {
        self.datastore.logs.get(index)
    }
    fn slice_logs_from(&self, index: u64) -> Result<Option<Logs>> {
        let logs = self.logs()?;
        Ok(logs.slice_from(index))
    }
    fn slice_logs_to(&self, index: u64) -> Result<Logs> {
        let logs = self.logs()?;
        Ok(logs.slice_to(index))
    }
}
