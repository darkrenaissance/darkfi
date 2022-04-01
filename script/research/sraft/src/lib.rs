use std::{collections::HashMap, io, net::SocketAddr, time::Duration};

use async_channel::{Receiver, Sender};
use async_std::{
    io::{ReadExt, WriteExt},
    net::{TcpListener, TcpStream},
    stream::StreamExt,
    sync::Mutex,
    task,
};
use borsh::{BorshDeserialize, BorshSerialize};
use futures::{select, FutureExt};
use lazy_static::lazy_static;
use log::{debug, error};
use rand::Rng;

mod method;
use crate::method::{HeartbeatArgs, HeartbeatReply, RaftMethod, VoteArgs, VoteReply};

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug)]
pub struct LogEntry {
    log_term: u64,
    log_index: u64,
    log_data: Vec<u8>,
}

pub struct LogStore(pub Vec<LogEntry>);

impl LogStore {
    fn get_last_index(&self) -> u64 {
        let rlen = self.0.len();
        if rlen == 0 {
            return 0
        }

        self.0[rlen - 1].log_index
    }
}

lazy_static! {
    pub static ref LOG_STORE: Mutex<LogStore> = Mutex::new(LogStore(vec![]));
    // This is used for heartbeats
    pub static ref HEARTBEAT_CHAN: (Sender<bool>, Receiver<bool>) = async_channel::unbounded();
    // This is used to let our node know when it has become a leader
    pub static ref TOLEADER_CHAN: (Sender<bool>, Receiver<bool>) = async_channel::unbounded();

    pub static ref STATE: Mutex<State> = Mutex::new(State::new());
}

#[derive(Default)]
pub struct State {
    pub current_term: u64,
    pub voted_for: u64,
    pub vote_count: u64,

    pub commit_index: u64,
    pub _last_applied: u64,

    pub next_index: Vec<u64>,
    pub match_index: Vec<u64>,
}

impl State {
    pub fn new() -> Self {
        Self {
            current_term: 0,
            voted_for: 0,
            vote_count: 0,
            commit_index: 0,
            _last_applied: 0,
            next_index: vec![],
            match_index: vec![],
        }
    }
}

pub enum Role {
    Follower,
    Candidate,
    Leader,
}

pub struct Raft {
    pub peers: HashMap<u64, SocketAddr>,
    node_id: u64,
    role: Role,
}

impl Raft {
    pub fn new(node_id: u64) -> Self {
        Self { peers: Default::default(), node_id, role: Role::Follower }
    }

    pub async fn start(&mut self) {
        debug!("Raft::start()");
        self.role = Role::Follower;

        let mut state = STATE.lock().await;
        state.current_term = 0;
        state.voted_for = 0;
        drop(state);

        let mut rng = rand::thread_rng();

        loop {
            let delay = Duration::from_millis(rng.gen_range(0..200) + 300);

            match self.role {
                Role::Follower => {
                    select! {
                        _ = HEARTBEAT_CHAN.1.recv().fuse() => {
                            debug!("[FOLLOWER] Raft::start(): follower_{} got heartbeat", self.node_id);
                        }
                        _ = task::sleep(delay).fuse() => {
                            debug!("[FOLLOWER] Raft::start(): follower_{} timeout", self.node_id);
                            self.role = Role::Candidate;
                        }
                    }
                }

                Role::Candidate => {
                    debug!("[CANDIDATE] Raft::start(): peer_{} is now a candidate", self.node_id);
                    let mut state = STATE.lock().await;
                    state.current_term += 1;
                    state.voted_for = self.node_id;
                    state.vote_count = 1;
                    drop(state);

                    // TODO: In background
                    debug!("[CANDIDATE] Raft::start(): broadcasting request_vote");
                    self.broadcast_request_vote().await;

                    select! {
                        _ = task::sleep(delay).fuse() => {
                            debug!("[CANDIDATE] Raft::start(): Timeout as candidate, becoming a follower");
                            self.role = Role::Follower;
                        }
                        _ = TOLEADER_CHAN.1.recv().fuse() => {
                            debug!("[CANDIDATE] Raft::start(): We are now the leader");
                            self.role = Role::Leader;

                            let mut state = STATE.lock().await;
                            state.next_index = vec![1_u64; self.peers.len()];
                            state.match_index = vec![0_u64; self.peers.len()];
                            drop(state);

                            // TODO: In background
                            let t = task::spawn(async {
                                let mut i = 0;
                                loop {
                                    debug!("[CANDIDATE] Raft::start(): Appending data in bg loop");
                                    i += 1;
                                    let state = STATE.lock().await;
                                    let logentry = LogEntry {
                                        log_term: state.current_term,
                                        log_index: i,
                                        log_data: format!("user send: {}", i).as_bytes().to_vec(),
                                    };
                                    drop(state);

                                    debug!("[CANDIDATE] Raft::start(): Acquiring logstore lock in bg loop");
                                    let mut logstore = LOG_STORE.lock().await;
                                    logstore.0.push(logentry);
                                    drop(logstore);
                                    debug!("[CANDIDATE] Raft::start(): Dropped logstore lock in bg loop");
                                    task::sleep(Duration::from_secs(3)).await;
                                }
                            });
                        }
                    }
                }

                Role::Leader => {
                    debug!("[LEADER] Raft::start(): Broadcasting heartbeat as leader");
                    self.broadcast_heartbeat().await;
                    task::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }

    async fn broadcast_request_vote(&mut self) {
        debug!("Raft::broadcast_request_vote()");
        let state = STATE.lock().await;
        let args = VoteArgs { term: state.current_term, candidate_id: self.node_id };
        drop(state);

        // TODO: Do this concurrently
        for i in self.peers.clone() {
            debug!("Raft::broadcast_request_vote(): Sending req to peer {}", i.1);
            match self.send_request_vote(i.0, args.clone()).await {
                Ok(v) => debug!("Raft::broadcast_request_vote(): Got reply: {:?}", v),
                Err(e) => {
                    error!("Raft::broadcast_request_vote(): Failed vote to peer {}, ({})", i.1, e);
                    continue
                }
            };
        }
    }

    async fn send_request_vote(
        &mut self,
        node_id: u64,
        args: VoteArgs,
    ) -> Result<VoteReply, io::Error> {
        debug!("Raft::send_request_vote()");
        let addr = self.peers[&node_id];

        let method = RaftMethod::Vote(args);
        let payload = method.try_to_vec().unwrap();

        debug!("Raft::send_request_vote(): Connecting to peer_{}", node_id);
        let mut stream = TcpStream::connect(addr).await?;
        debug!("Raft::send_request_vote(): Writing to stream");
        stream.write_all(&payload).await?;
        debug!("Raft::send_request_vote(): Wrote to stream");

        debug!("Raft::send_request_vote(): Reading from stream");
        let mut buf = vec![0_u8; 4096];
        stream.read(&mut buf).await?;
        debug!("Raft::send_request_vote(): Read from stream");

        let reply = try_from_slice_unchecked::<VoteReply>(&buf)?;
        let mut state = STATE.lock().await;
        if reply.term > state.current_term {
            debug!("Raft::send_request_vote(): reply.term > state.current_term");
            state.current_term = reply.term;
            state.voted_for = 0;
            drop(state);
            self.role = Role::Follower;
            return Ok(reply)
        }
        drop(state);

        if reply.vote_granted {
            debug!("Raft::send_request_vote(): reply.vote_granted == true");
            let mut state = STATE.lock().await;
            state.vote_count += 1;
            drop(state);
        }

        let state = STATE.lock().await;
        if state.vote_count >= (self.peers.len() / 2 + 1).try_into().unwrap() {
            debug!("Raft::send_request_vote(): Elected for leader");
            TOLEADER_CHAN.0.send(true).await.unwrap();
        }
        drop(state);

        Ok(reply)
    }

    async fn broadcast_heartbeat(&mut self) {
        debug!("[LEADER] Raft::broadcast_heartbeat()");

        for i in self.peers.clone() {
            let state = STATE.lock().await;
            let mut args = HeartbeatArgs {
                term: state.current_term,
                leader_id: self.node_id,
                prev_log_index: 0,
                prev_log_term: 0,
                entries: vec![],
                leader_commit: state.commit_index,
            };

            let prev_log_index = state.next_index[i.0 as usize] - 1;
            drop(state);

            debug!("[LEADER] Raft::broadcast_heartbeat(): Acquiring lock on LOG_STORE");
            let logstore = LOG_STORE.lock().await;
            if logstore.get_last_index() > prev_log_index {
                args.prev_log_index = prev_log_index;
                args.prev_log_term = logstore.0[prev_log_index as usize].log_term;
                args.entries = logstore.0[prev_log_index as usize..].to_vec();
                drop(logstore);
                debug!("[LEADER] Raft::broadcast_heartbeat(): Dropped lock on LOG_STORE");
                debug!("[LEADER] Raft::broadcast_heartbeat(): Send entries: {:?}", args.entries);
            }

            // TODO: Run in background
            match self.send_heartbeat(i.0, args).await {
                Ok(v) => debug!("[LEADER] Raft::broadcast_heartbeat(): Got reply: {:?}", v),
                Err(e) => {
                    error!(
                        "[LEADER] Raft::broadcast_heartbeat(): Failed heartbeat to peer_{} ({})",
                        i.0, e
                    );
                    continue
                }
            };
        }
    }

    async fn send_heartbeat(
        &mut self,
        node_id: u64,
        args: HeartbeatArgs,
    ) -> Result<HeartbeatReply, io::Error> {
        debug!("Raft::send_heartbeat({}, {:?}", node_id, args);
        let addr = self.peers[&node_id];

        let method = RaftMethod::Heartbeat(args);
        let payload = method.try_to_vec()?;

        debug!("Raft::send_heartbeat(): Connecting to peer_{}", node_id);
        let mut stream = TcpStream::connect(addr).await?;
        debug!("Raft::send_heartbeat(): Writing to stream");
        stream.write_all(&payload).await?;
        debug!("Raft::send_heartbeat(): Wrote to stream");

        debug!("Raft::send_heartbeat(): Reading from stream");
        let mut buf = vec![0_u8; 4096];
        stream.read(&mut buf).await?;
        debug!("Raft::send_heartbeat(): Read from stream");

        let reply = try_from_slice_unchecked::<HeartbeatReply>(&buf)?;

        let mut state = STATE.lock().await;
        if reply.success {
            debug!("Raft::send_heartbeat(): Got success reply");
            if reply.next_index > 0 {
                state.next_index[node_id as usize] = reply.next_index;
                state.match_index[node_id as usize] = reply.next_index - 1;
            }
        } else if reply.term > state.current_term {
            debug!("Raft::send_heartbeat(): reply.term > state.current_term");
            state.current_term = reply.term;
            state.voted_for = 0;
            self.role = Role::Follower;
        }
        drop(state);

        Ok(reply)
    }
}

pub struct RaftRpc(pub SocketAddr);

impl RaftRpc {
    pub async fn start(&self) {
        debug!("RaftRpc::start()");

        debug!("RaftRpc::start(): Binding to {}", self.0);
        let listener = TcpListener::bind(self.0).await.unwrap();
        let mut incoming = listener.incoming();

        while let Some(stream) = incoming.next().await {
            debug!("RaftRpc::start(): Got RPC request");
            let stream = stream.unwrap();
            let (reader, writer) = &mut (&stream, &stream);

            debug!("RaftRpc::start(): Reading from reader...");
            let mut buf = vec![0_u8; 4096];
            reader.read(&mut buf).await.unwrap();
            debug!("RaftRpc::start(): Read from reader");

            match try_from_slice_unchecked::<RaftMethod>(&buf).unwrap() {
                RaftMethod::Vote(args) => {
                    debug!("RaftRpc::start(): Got RaftMethod::Vote");
                    let reply = self.request_vote(args).await;
                    let payload = reply.try_to_vec().unwrap();

                    debug!("RaftRpc::start(): Vote: Writing to writer...");
                    writer.write_all(&payload).await.unwrap();
                    debug!("RaftRpc::start(): Vote: Wrote to writer");
                }

                RaftMethod::Heartbeat(args) => {
                    debug!("RaftRpc::start(): Got RaftMethod::Heartbeat");
                    let reply = self.heartbeat(args).await;
                    let payload = reply.try_to_vec().unwrap();

                    debug!("RaftRpc::start(): Heartbeat: Writing to writer...");
                    writer.write_all(&payload).await.unwrap();
                    debug!("RaftRpc::start(): Heartbeat: Wrote to writer");
                }
            }
        }
    }

    async fn request_vote(&self, args: VoteArgs) -> VoteReply {
        debug!("RaftRpc::request_vote()");
        let mut reply = VoteReply { term: 0, vote_granted: false };

        debug!("RaftRpc::request_vote(): Acquiring state lock");
        let mut state = STATE.lock().await;
        debug!("RaftRpc::request_vote(): Got lock");

        if args.term < state.current_term {
            reply.term = state.current_term;
            drop(state);
            reply.vote_granted = false;
            return reply
        }

        if state.voted_for == 0 {
            state.current_term = args.term;
            state.voted_for = args.candidate_id;
            drop(state);
            reply.term = args.term;
            reply.vote_granted = true;
            return reply
        }

        drop(state);
        reply
    }

    async fn heartbeat(&self, args: HeartbeatArgs) -> HeartbeatReply {
        debug!("RaftRpc::heartbeat()");
        let mut reply = HeartbeatReply { success: false, term: 0, next_index: 0 };

        debug!("RaftRpc::heartbeat(): Acquiring state lock");
        let state = STATE.lock().await;
        debug!("RaftRpc::heartbeat(): Got state lock");
        let current_term = state.current_term;
        drop(state);
        debug!("RaftRpc::heartbeat(): Dropped state lock");

        if args.term < current_term {
            reply.success = false;
            reply.term = current_term;
            return reply
        }

        debug!("RaftRpc::heartbeat(): Sending to channel");
        HEARTBEAT_CHAN.0.send(true).await.unwrap();
        debug!("RaftRpc::heartbeat(): Sent to channel");

        if args.entries.is_empty() {
            reply.success = true;
            reply.term = current_term;
            return reply
        }

        debug!("RaftRpc::heartbeat(): Acquiring logstore lock");
        let mut logstore = LOG_STORE.lock().await;
        debug!("RaftRpc::heartbeat(): Got logstore lock");
        if args.prev_log_index > logstore.get_last_index() {
            reply.success = false;
            reply.term = current_term;
            reply.next_index = logstore.get_last_index() + 1;
            drop(logstore);
            return reply
        }

        logstore.0.extend_from_slice(&args.entries);
        reply.next_index = logstore.get_last_index() + 1;
        drop(logstore);
        debug!("RaftRpc::heartbeat(): Dropped logstore lock");

        reply.success = true;
        reply.term = current_term;

        reply
    }
}

fn try_from_slice_unchecked<T: BorshDeserialize>(data: &[u8]) -> Result<T, io::Error> {
    let mut data_mut = data;
    let result = T::deserialize(&mut data_mut)?;
    Ok(result)
}
