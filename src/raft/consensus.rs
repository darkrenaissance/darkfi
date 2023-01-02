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

use std::{collections::HashMap, time::Duration};

use async_std::{
    sync::{Arc, Mutex},
    task::sleep,
};
use chrono::Utc;
use darkfi_serial::{deserialize, serialize, Decodable, Encodable};
use futures::{select, FutureExt};
use log::{debug, error, warn};
use rand::{distributions::Alphanumeric, rngs::OsRng, thread_rng, Rng, RngCore};
use smol::Executor;

use crate::{net, Error, Result};

use super::{
    p2p_send_loop,
    primitives::{
        BroadcastMsgRequest, Channel, Log, LogRequest, LogResponse, Logs, MapLength, NetMsg,
        NetMsgMethod, NodeId, NodeIdMsg, Role, Sender, VoteRequest, VoteResponse,
    },
    prune_map, DataStore, RaftSettings,
};

async fn send_loop(sender: smol::channel::Sender<()>, timeout: Duration) -> Result<()> {
    loop {
        sleep(timeout).await;
        sender.send(()).await?;
    }
}

pub fn gen_id(len: usize) -> String {
    thread_rng().sample_iter(&Alphanumeric).take(len).map(char::from).collect()
}

pub struct Raft<T> {
    id: NodeId,

    pub(super) role: Role,

    pub(super) current_leader: NodeId,

    pub(super) votes_received: Vec<NodeId>,

    pub(super) sent_length: MapLength,
    pub(super) acked_length: MapLength,

    pub(super) nodes: Arc<Mutex<HashMap<NodeId, i64>>>,

    pub(super) last_term: u64,

    pub(super) last_heartbeat: i64,

    p2p_sender: Sender,

    msgs_channel: Channel<T>,
    commits_channel: Channel<T>,

    datastore: DataStore<T>,

    seen_msgs: Arc<Mutex<HashMap<String, i64>>>,

    pub(super) settings: RaftSettings,

    pending_msgs: Vec<T>,
}

impl<T: Decodable + Encodable + Clone> Raft<T> {
    pub fn new(
        settings: RaftSettings,
        seen_msgs: Arc<Mutex<HashMap<String, i64>>>,
    ) -> Result<Self> {
        if settings.datastore_path.to_str().is_none() {
            error!(target: "raft::consensus", "datastore path is incorrect");
            return Err(Error::ParseFailed("unable to parse pathbuf to str"))
        };

        let datastore = DataStore::new(settings.datastore_path.to_str().unwrap())?;

        // broadcasting channels
        let msgs_channel = smol::channel::unbounded::<T>();
        let commits_channel = smol::channel::unbounded::<T>();

        let p2p_sender = smol::channel::unbounded::<NetMsg>();

        let id = match datastore.id.get_last()? {
            Some(_id) => _id,
            None => {
                // FIXME: This should be a big number, like a hash.
                let id = NodeId(gen_id(30));
                datastore.id.insert(&id)?;
                id
            }
        };

        let role = Role::Follower;

        Ok(Self {
            id,
            role,
            current_leader: NodeId("".into()),
            votes_received: vec![],
            sent_length: MapLength(HashMap::default()),
            acked_length: MapLength(HashMap::default()),
            nodes: Arc::new(Mutex::new(HashMap::default())),
            last_term: 0,
            last_heartbeat: Utc::now().timestamp(),
            p2p_sender,
            msgs_channel,
            commits_channel,
            datastore,
            seen_msgs,
            settings,
            pending_msgs: vec![],
        })
    }

    ///  
    ///  Run raft consensus and wait stop_signal channel to terminate
    ///
    pub async fn run(
        &mut self,
        p2p: net::P2pPtr,
        p2p_recv_channel: smol::channel::Receiver<NetMsg>,
        executor: Arc<Executor<'_>>,
        stop_signal: smol::channel::Receiver<()>,
    ) -> Result<()> {
        let p2p_send_task = executor.spawn(p2p_send_loop(self.p2p_sender.1.clone(), p2p.clone()));

        let prune_seen_messages_task = executor
            .spawn(prune_map::<String>(self.seen_msgs.clone(), self.settings.prun_duration));

        let prune_nodes_id_task =
            executor.spawn(prune_map::<NodeId>(self.nodes.clone(), self.settings.prun_duration));

        let (id_sx, id_rv) = smol::channel::unbounded::<()>();
        let (heartbeat_sx, heartbeat_rv) = smol::channel::unbounded::<()>();
        let (timeout_sx, timeout_rv) = smol::channel::unbounded::<()>();

        let id_timeout = Duration::from_secs(self.settings.id_timeout);
        let send_id_task = executor.spawn(send_loop(id_sx, id_timeout));

        let heartbeat_timeout = Duration::from_millis(self.settings.heartbeat_timeout);
        let send_heartbeat_task = executor.spawn(send_loop(heartbeat_sx, heartbeat_timeout));

        let rng = &mut OsRng;
        let timeout =
            Duration::from_secs(rng.gen_range(0..self.settings.timeout) + self.settings.timeout);
        let send_timeout_task = executor.spawn(send_loop(timeout_sx, timeout));

        let broadcast_msg_rv = self.msgs_channel.1.clone();

        loop {
            let mut result = select! {
                m =  p2p_recv_channel.recv().fuse() => self.handle_method(m?).await,
                m =  broadcast_msg_rv.recv().fuse() => self.broadcast_msg(&m?,None).await,
                _ =  id_rv.recv().fuse() => self.send_id_msg().await,
                _ = heartbeat_rv.recv().fuse() => self.send_heartbeat().await,
                _ = timeout_rv.recv().fuse() => self.send_vote_request().await,
                _ = stop_signal.recv().fuse() => break,
            };

            // send pending messages
            if !self.pending_msgs.is_empty() && self.role != Role::Candidate {
                let pending_msgs = self.pending_msgs.clone();
                for m in &pending_msgs {
                    result = self.broadcast_msg(m, None).await;
                }
                self.pending_msgs = vec![];
            }

            if let Err(e) = result {
                warn!(target: "raft::consensus", "warn: {}", e);
            }
        }

        warn!(target: "raft::consensus", "Raft Terminating...");
        p2p_send_task.cancel().await;
        prune_seen_messages_task.cancel().await;
        prune_nodes_id_task.cancel().await;
        send_id_task.cancel().await;
        send_heartbeat_task.cancel().await;
        send_timeout_task.cancel().await;
        self.datastore.flush().await?;
        Ok(())
    }

    ///  
    /// Return async receiver channel which can be used to receive T Messages
    /// from raft consensus
    ///
    pub fn receiver(&self) -> smol::channel::Receiver<T> {
        self.commits_channel.1.clone()
    }

    ///  
    /// Return async sender channel which can be used to broadcast T Messages
    /// to raft consensus
    ///
    pub fn sender(&self) -> smol::channel::Sender<T> {
        self.msgs_channel.0.clone()
    }

    ///  
    /// Return the raft node id
    ///
    pub fn id(&self) -> NodeId {
        self.id.clone()
    }

    async fn send_id_msg(&self) -> Result<()> {
        let id_msg = serialize(&NodeIdMsg { id: self.id.clone() });
        self.send(None, &id_msg, NetMsgMethod::NodeIdMsg, None).await?;
        Ok(())
    }

    async fn broadcast_msg(&mut self, msg: &T, msg_id: Option<u64>) -> Result<()> {
        match self.role {
            Role::Leader => {
                let msg = serialize(msg);
                let log = Log { msg, term: self.current_term()? };
                self.push_log(&log)?;
                self.acked_length.insert(&self.id, self.logs_len());
            }
            Role::Follower => {
                let b_msg = BroadcastMsgRequest(serialize(msg));
                self.send(
                    Some(self.current_leader.clone()),
                    &serialize(&b_msg),
                    NetMsgMethod::BroadcastRequest,
                    msg_id,
                )
                .await?;
            }
            Role::Candidate => {
                self.pending_msgs.push(msg.clone());
            }
        }

        debug!(target: "raft::consensus", "Role: {:?} Id: {:?}, broadcast a msg id: {:?} ", self.role, self.id, msg_id);

        Ok(())
    }

    async fn handle_method(&mut self, msg: NetMsg) -> Result<()> {
        match msg.method {
            NetMsgMethod::LogResponse => {
                let lr: LogResponse = deserialize(&msg.payload)?;
                self.receive_log_response(lr).await?;
            }
            NetMsgMethod::LogRequest => {
                self.last_heartbeat = Utc::now().timestamp();
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
            NetMsgMethod::NodeIdMsg => {
                let node_id_msg: NodeIdMsg = deserialize(&msg.payload)?;
                if node_id_msg.id != self.id {
                    self.nodes.lock().await.insert(node_id_msg.id, Utc::now().timestamp());
                }
            }
        }

        debug!(target: "raft::consensus", "Role: {:?} Id: {:?}, receive a msg with id: {}  recipient_id: {:?} method: {:?} ",
               self.role, self.id, msg.id, &msg.recipient_id, &msg.method);
        Ok(())
    }

    pub(super) async fn send(
        &self,
        recipient_id: Option<NodeId>,
        payload: &[u8],
        method: NetMsgMethod,
        msg_id: Option<u64>,
    ) -> Result<()> {
        let random_id = if msg_id.is_some() { msg_id.unwrap() } else { OsRng.next_u64() };

        debug!(target: "raft::consensus","Role: {:?} Id: {:?}, send a msg with id: {}  recipient_id: {:?} method: {:?} ",
               self.role, self.id, random_id, &recipient_id, &method);

        let net_msg = NetMsg { id: random_id, recipient_id, payload: payload.to_vec(), method };
        self.seen_msgs.lock().await.insert(random_id.to_string(), Utc::now().timestamp());
        self.p2p_sender.0.send(net_msg).await?;

        Ok(())
    }

    pub(super) fn reset_last_term(&mut self) -> Result<()> {
        self.last_term = 0;

        if let Some(log) = self.last_log()? {
            self.last_term = log.term;
        }

        Ok(())
    }

    pub(super) fn set_current_term(&mut self, i: &u64) -> Result<()> {
        self.datastore.current_term.insert(i)
    }

    pub(super) fn set_voted_for(&mut self, i: &Option<NodeId>) -> Result<()> {
        self.datastore.voted_for.insert(i)
    }

    pub(super) async fn push_commit(&mut self, commit: &[u8]) -> Result<()> {
        let commit: T = deserialize(commit)?;
        self.commits_channel.0.send(commit.clone()).await?;
        self.datastore.commits.insert(&commit)
    }

    pub(super) fn push_log(&mut self, log: &Log) -> Result<()> {
        self.datastore.logs.insert(log)
    }

    pub(super) fn push_logs(&mut self, logs: &Logs) -> Result<()> {
        self.datastore.logs.wipe_insert_all(&logs.to_vec())
    }

    pub(super) fn current_term(&self) -> Result<u64> {
        Ok(self.datastore.current_term.get_last()?.unwrap_or(0))
    }

    pub(super) fn voted_for(&self) -> Result<Option<NodeId>> {
        Ok(self.datastore.voted_for.get_last()?.flatten())
    }

    pub(super) fn commits_len(&self) -> u64 {
        self.datastore.commits.len()
    }

    fn logs(&self) -> Result<Logs> {
        Ok(Logs(self.datastore.logs.get_all()?))
    }

    pub(super) fn logs_len(&self) -> u64 {
        self.datastore.logs.len()
    }

    fn last_log(&self) -> Result<Option<Log>> {
        self.datastore.logs.get_last()
    }

    pub(super) fn get_log(&self, index: u64) -> Result<Log> {
        self.datastore.logs.get(index)
    }

    pub(super) fn slice_logs_from(&self, index: u64) -> Result<Option<Logs>> {
        let logs = self.logs()?;
        Ok(logs.slice_from(index))
    }

    pub(super) fn slice_logs_to(&self, index: u64) -> Result<Logs> {
        let logs = self.logs()?;
        Ok(logs.slice_to(index))
    }
}
