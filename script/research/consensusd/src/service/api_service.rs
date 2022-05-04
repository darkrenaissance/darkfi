use std::path::PathBuf;

use async_executor::Executor;
use async_std::sync::Arc;
use async_trait::async_trait;
use log::debug;
use serde::Serialize;
use serde_json::{json, Value};

use darkfi::{
    crypto::keypair::PublicKey,
    rpc::{
        jsonrpc,
        jsonrpc::{
            response as jsonresp,
            ErrorCode::{InvalidParams, MethodNotFound, ServerError},
            JsonRequest, JsonResult,
        },
        rpcserver::RequestHandler,
    },
    Result,
};

use super::{state::State, vote::Vote};

/// This struct represent the Consensus service RPC daemon.
#[derive(Serialize)]
pub struct APIService {
    id: u64,
    state_path: PathBuf,
}

impl APIService {
    pub fn new(id: u64, state_path: PathBuf) -> Result<Arc<APIService>> {
        match State::reset(id, &state_path) {
            Err(e) => return Err(e),
            _ => (),
        }

        Ok(Arc::new(APIService { id, state_path }))
    }

    /// RPCAPI:
    /// Hello world example.
    /// --> {"jsonrpc": "2.0", "method": "say_hello", "params": [], "id": 0}
    /// <-- {"jsonrpc": "2.0", "result": "hello world", "id": 0}
    async fn say_hello(&self) -> JsonResult {
        JsonResult::Resp(jsonresp(json!("hello world"), serde_json::to_value(self.id).unwrap()))
    }

    /// RPCAPI:
    /// Node receives a transaction and stores it in its current state.
    /// --> {"jsonrpc": "2.0", "method": "receive_tx", "params": ["tx"], "id": 0}
    /// <-- {"jsonrpc": "2.0", "result": true, "id": 0}
    async fn receive_tx(&self, params: Value) -> JsonResult {
        let args = params.as_array().unwrap();

        if args.len() != 1 {
            return jsonrpc::error(InvalidParams, None, serde_json::to_value(self.id).unwrap())
                .into()
        }

        let mut state = State::load_current_state(self.id, &self.state_path).unwrap();
        let tx = String::from(args[0].as_str().unwrap());

        let result = || -> Result<()> {
            state.append_tx(tx);
            state.save(&self.state_path)?;
            Ok(())
        };

        match result() {
            Ok(()) => {
                JsonResult::Resp(jsonresp(json!(true), serde_json::to_value(self.id).unwrap()))
            }
            Err(e) => jsonrpc::error(
                ServerError(-32603),
                Some(e.to_string()),
                serde_json::to_value(self.id).unwrap(),
            )
            .into(),
        }
    }

    /// RPCAPI:
    /// Node checks if its the current slot leader and generates the slot Block (represented as a Vote structure).
    /// --> {"jsonrpc": "2.0", "method": "consensus_task", "params": [1], "id": 0}
    /// <-- {"jsonrpc": "2.0", "result": [PublicKey, Vote], "id": 0}
    /// Missing: 1, This should be a scheduled task.
    ///          2. Nodes count not from request.
    ///          3. Proposed block broadcast.
    async fn consensus_task(&self, params: Value) -> JsonResult {
        let args = params.as_array().unwrap();

        if args.len() != 1 {
            return jsonrpc::error(InvalidParams, None, serde_json::to_value(self.id).unwrap())
                .into()
        }

        let state = State::load_current_state(self.id, &self.state_path).unwrap();
        let nodes_count = args[0].as_u64().unwrap();

        let result = || -> Result<_> {
            let proposed_block =
                if state.check_if_epoch_leader(nodes_count) { state.propose_block() } else { None };
            Ok(proposed_block)
        };

        match result() {
            Ok(x) => {
                if x.is_none() {
                    JsonResult::Resp(jsonresp(
                        json!("Node is not the epoch leader"),
                        serde_json::to_value(self.id).unwrap(),
                    ))
                } else {
                    // Missing: Proposed block broadcast.
                    JsonResult::Resp(jsonresp(
                        json!((state.public_key, x)),
                        serde_json::to_value(self.id).unwrap(),
                    ))
                }
            }
            Err(e) => jsonrpc::error(
                ServerError(-32603),
                Some(e.to_string()),
                serde_json::to_value(self.id).unwrap(),
            )
            .into(),
        }
    }

    /// RPCAPI:
    /// Node receives a proposed block, verifies it and stores it in its current state.
    /// --> {"jsonrpc": "2.0", "method": "receive_proposed_block", "params": [PublicKey, Vote, 1], "id": 0}
    /// <-- {"jsonrpc": "2.0", "result": [PublicKey, Vote], "id": 0}
    /// Missing: 1. Nodes count not from request.
    ///          2. Vote broadcast.
    async fn receive_proposed_block(&self, params: Value) -> JsonResult {
        let args = params.as_array().unwrap();

        if args.len() != 3 {
            return jsonrpc::error(InvalidParams, None, serde_json::to_value(self.id).unwrap())
                .into()
        }

        let mut state = State::load_current_state(self.id, &self.state_path).unwrap();
        let proposer_public_key: PublicKey = serde_json::from_value(args[0].clone()).unwrap();
        let proposed_block: Vote = serde_json::from_value(args[1].clone()).unwrap();
        let nodes_count = args[2].as_u64().unwrap();

        let mut result = || -> Result<_> {
            let vote =
                state.receive_proposed_block(&proposer_public_key, &proposed_block, nodes_count);
            if vote.is_some() {
                state.save(&self.state_path)?;
            }
            Ok(vote)
        };

        match result() {
            Ok(x) => {
                if x.is_none() {
                    JsonResult::Resp(jsonresp(
                        json!("Node did not vote for the proposed block."),
                        serde_json::to_value(self.id).unwrap(),
                    ))
                } else {
                    // Missing: Vote broadcast.
                    JsonResult::Resp(jsonresp(
                        json!((state.public_key, x)),
                        serde_json::to_value(self.id).unwrap(),
                    ))
                }
            }
            Err(e) => jsonrpc::error(
                ServerError(-32603),
                Some(e.to_string()),
                serde_json::to_value(self.id).unwrap(),
            )
            .into(),
        }
    }

    /// RPCAPI:
    /// Node receives a block vote and perform the consensus protocol corresponding functions, based on its current state.
    /// --> {"jsonrpc": "2.0", "method": "receive_vote", "params": [PublicKey, Vote, 1], "id": 0}
    /// <-- {"jsonrpc": "2.0", "result": true, "id": 0}
    /// Missing: 1. Nodes count not from request.
    async fn receive_vote(&self, params: Value) -> JsonResult {
        let args = params.as_array().unwrap();

        if args.len() != 3 {
            return jsonrpc::error(InvalidParams, None, serde_json::to_value(self.id).unwrap())
                .into()
        }

        let mut state = State::load_current_state(self.id, &self.state_path).unwrap();
        let voter_public_key: PublicKey = serde_json::from_value(args[0].clone()).unwrap();
        let vote: Vote = serde_json::from_value(args[1].clone()).unwrap();
        let nodes_count = args[2].as_u64().unwrap() as usize;

        let mut result = || -> Result<()> {
            state.receive_vote(&voter_public_key, &vote, nodes_count);
            state.save(&self.state_path)?;
            Ok(())
        };

        match result() {
            Ok(()) => {
                JsonResult::Resp(jsonresp(json!(true), serde_json::to_value(self.id).unwrap()))
            }
            Err(e) => jsonrpc::error(
                ServerError(-32603),
                Some(e.to_string()),
                serde_json::to_value(self.id).unwrap(),
            )
            .into(),
        }
    }
}

#[async_trait]
impl RequestHandler for APIService {
    /// RPC methods configuration.
    async fn handle_request(&self, req: JsonRequest, _executor: Arc<Executor<'_>>) -> JsonResult {
        if req.id.as_u64().unwrap() != self.id || req.params.as_array().is_none() {
            return jsonrpc::error(InvalidParams, None, serde_json::to_value(self.id).unwrap())
                .into()
        }

        debug!(target: "RPC", "--> {}", serde_json::to_string(&req).unwrap());

        return match req.method.as_str() {
            Some("say_hello") => self.say_hello().await,
            Some("receive_tx") => self.receive_tx(req.params).await,
            Some("consensus_task") => self.consensus_task(req.params).await,
            Some("receive_proposed_block") => self.receive_proposed_block(req.params).await,
            Some("receive_vote") => self.receive_vote(req.params).await,
            Some(_) | None => {
                jsonrpc::error(MethodNotFound, None, serde_json::to_value(self.id).unwrap()).into()
            }
        }
    }
}
