/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs::create_dir_all,
    path::PathBuf,
    sync::Arc,
};

use async_trait::async_trait;
use sled_overlay::sled;
use smol::lock::{Mutex, MutexGuard, RwLock};
use tinyjson::JsonValue;
use tracing::{debug, info, warn};

use darkfi::{
    event_graph::{
        rln::{prepare_slash_proof_request, RLNNode, RlnProver, SlashBlob, GENESIS_USER_MSG_LIMIT},
        Event, EventGraphPtr,
    },
    net,
    rpc::{
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResult, JsonSubscriber},
        p2p_method::HandlerP2p,
        server::RequestHandler,
    },
    system::StoppableTaskPtr,
    util::{memory::log_memory, path::expand_path, time::Timestamp},
    Error,
};

use darkfi_sdk::{crypto::pasta_prelude::PrimeField, pasta::pallas};
use darkfi_serial::{deserialize_async, serialize_async};

use taud::{
    error::{to_json_result, TaudError, TaudResult},
    genesis_commits::is_pregenerated_commitment,
    month_tasks::MonthTasks,
    rln::{RlnIdentity, ACCOUNTS_DB_PREFIX, ACCOUNTS_DEFAULT_TREE, ACCOUNTS_KEY_RLN_IDENTITY},
    task_info::{Comment, TaskInfo},
    util::set_event,
};

use crate::Workspace;

const MAX_ACCOUNT_NAME_LEN: usize = 24;

pub struct JsonRpcInterface {
    dataset_path: PathBuf,
    notify_queue_sender: smol::channel::Sender<TaskInfo>,
    nickname: String,
    workspace: Mutex<String>,
    workspaces: Arc<BTreeMap<String, Workspace>>,
    p2p: net::P2pPtr,
    event_graph: EventGraphPtr,
    dnet_sub: JsonSubscriber,
    deg_sub: JsonSubscriber,
    rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
    sled_db: sled::Db,
    rln_identity: Arc<RwLock<Option<RlnIdentity>>>,
}

#[async_trait]
impl RequestHandler<()> for JsonRpcInterface {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        let rep = match req.method.as_str() {
            "add" => self.add(req.params).await,
            "get_ref_ids" => self.get_ref_ids(req.params).await,
            "get_archive_ref_ids" => self.get_archive_ref_ids(req.params).await,
            "modify" => self.modify(req.params).await,
            "set_state" => self.set_state(req.params).await,
            "set_comment" => self.set_comment(req.params).await,
            "get_task_by_ref_id" => self.get_task_by_ref_id(req.params).await,
            "switch_ws" => self.switch_ws(req.params).await,
            "get_ws" => self.get_ws(req.params).await,
            "export" => self.export_to(req.params).await,
            "import" => self.import_from(req.params).await,
            "fetch_deactive_tasks" => self.fetch_deactive_tasks(req.params).await,
            "fetch_archive_task" => self.fetch_archive_task(req.params).await,

            "rln_register" => self.rln_register(req.params).await,
            "rln_info" => self.rln_info(req.params).await,
            "rln_set" => self.rln_set(req.params).await,
            "rln_deregister" => self.rln_deregister(req.params).await,
            "rln_slash" => self.rln_slash(req.params).await,

            "ping" => return self.pong(req.id, req.params).await,
            "dnet.subscribe_events" => return self.dnet_subscribe_events(req.id, req.params).await,
            "dnet.switch" => self.dnet_switch(req.params).await,

            "deg.switch" => self.deg_switch(req.id, req.params).await,
            "deg.subscribe_events" => return self.deg_subscribe_events(req.id, req.params).await,
            "eventgraph.get_info" => return self.eg_get_info(req.id, req.params).await,

            "p2p.get_info" => return self.p2p_get_info(req.id, req.params).await,
            _ => return JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
        };

        to_json_result(rep, req.id)
    }

    async fn connections_mut(&self) -> MutexGuard<'life0, HashSet<StoppableTaskPtr>> {
        self.rpc_connections.lock().await
    }
}

impl HandlerP2p for JsonRpcInterface {
    fn p2p(&self) -> net::P2pPtr {
        self.p2p.clone()
    }
}

impl JsonRpcInterface {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        dataset_path: PathBuf,
        notify_queue_sender: smol::channel::Sender<TaskInfo>,
        nickname: String,
        workspace: String,
        workspaces: Arc<BTreeMap<String, Workspace>>,
        p2p: net::P2pPtr,
        event_graph: EventGraphPtr,
        dnet_sub: JsonSubscriber,
        deg_sub: JsonSubscriber,
        sled_db: sled::Db,
        rln_identity: Arc<RwLock<Option<RlnIdentity>>>,
    ) -> Self {
        let workspace = Mutex::new(workspace);
        Self {
            dataset_path,
            nickname,
            workspace,
            workspaces,
            notify_queue_sender,
            p2p,
            event_graph,
            rpc_connections: Mutex::new(HashSet::new()),
            dnet_sub,
            deg_sub,
            sled_db,
            rln_identity,
        }
    }

    // RPCAPI:
    // Activate or deactivate dnet in the P2P stack.
    // By sending `true`, dnet will be activated, and by sending `false` dnet will
    // be deactivated. Returns `true` on success.
    //
    // --> {"jsonrpc": "2.0", "method": "dnet_switch", "params": [true], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 42}
    async fn dnet_switch(&self, params: JsonValue) -> TaudResult<JsonValue> {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 1 || !params[0].is_bool() {
            return Err(TaudError::InvalidData("Invalid parameters".into()))
        }

        let switch = params[0].get::<bool>().unwrap();

        if *switch {
            self.p2p.dnet_enable();
        } else {
            self.p2p.dnet_disable();
        }

        Ok(JsonValue::Boolean(true))
    }

    // RPCAPI:
    // Initializes a subscription to p2p dnet events.
    // Once a subscription is established, `darkirc` will send JSON-RPC notifications of
    // new network events to the subscriber.
    //
    // --> {"jsonrpc": "2.0", "method": "dnet.subscribe_events", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "method": "dnet.subscribe_events", "params": [`event`]}
    pub async fn dnet_subscribe_events(&self, id: i64, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if !params.is_empty() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        self.dnet_sub.clone().into()
    }

    // RPCAPI:
    // Initializes a subscription to deg events.
    // Once a subscription is established, apps using eventgraph will send JSON-RPC notifications of
    // new eventgraph events to the subscriber.
    //
    // --> {"jsonrpc": "2.0", "method": "deg.subscribe_events", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "method": "deg.subscribe_events", "params": [`event`]}
    pub async fn deg_subscribe_events(&self, id: i64, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if !params.is_empty() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        self.deg_sub.clone().into()
    }

    // RPCAPI:
    // Activate or deactivate deg in the EVENTGRAPH.
    // By sending `true`, deg will be activated, and by sending `false` deg
    // will be deactivated. Returns `true` on success.
    //
    // --> {"jsonrpc": "2.0", "method": "deg.switch", "params": [true], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 42}
    async fn deg_switch(&self, _id: i64, params: JsonValue) -> TaudResult<JsonValue> {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 1 || !params[0].is_bool() {
            return Err(TaudError::InvalidData("Invalid parameters".into()))
        }

        let switch = params[0].get::<bool>().unwrap();

        if *switch {
            self.event_graph.deg_enable();
        } else {
            self.event_graph.deg_disable();
        }

        Ok(JsonValue::Boolean(true))
    }

    // RPCAPI:
    // Get EVENTGRAPH info.
    //
    // --> {"jsonrpc": "2.0", "method": "deg.switch", "params": [true], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 42}
    async fn eg_get_info(&self, id: i64, params: JsonValue) -> JsonResult {
        let params_ = params.get::<Vec<JsonValue>>().unwrap();
        if !params_.is_empty() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        self.event_graph.eventgraph_info(id, params).await
    }

    // RPCAPI:
    // Add new task and returns `true` upon success.
    // --> {"jsonrpc": "2.0", "method": "add",
    //      "params":
    //          [{
    //          "title": "..",
    //          "desc": "..",
    //          assign: [..],
    //          project: [..],
    //          "due": ..,
    //          "rank": ..
    //          }],
    //      "id": 1
    //      }
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    async fn add(&self, params: JsonValue) -> TaudResult<JsonValue> {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        debug!(target: "tau", "JsonRpc::add() params {params:?}");

        if !params[0].is_object() {
            return Err(TaudError::InvalidData("Invalid parameters".to_string()))
        }

        let params = params[0].get::<HashMap<String, JsonValue>>().unwrap();

        if params.len() != 10 {
            return Err(TaudError::InvalidData("Invalid parameters".to_string()))
        }

        let due = match params["due"] {
            JsonValue::Null => None,
            JsonValue::Number(numba) => Some(Timestamp::from_u64(numba as u64)),
            _ => return Err(TaudError::InvalidData("Invalid parameter \"due\"".to_string())),
        };

        let rank = match params["rank"] {
            JsonValue::Null => None,
            JsonValue::Number(numba) => Some(numba as f32),
            _ => return Err(TaudError::InvalidData("Invalid parameter \"rank\"".to_string())),
        };

        let bounty = match params["bounty"] {
            JsonValue::Null => None,
            JsonValue::Number(numba) => Some(numba as f32),
            _ => return Err(TaudError::InvalidData("Invalid parameter \"bounty\"".to_string())),
        };

        let tags = {
            let mut tags = vec![];

            for val in params["tags"].get::<Vec<JsonValue>>().unwrap().iter() {
                if let Some(tag) = val.get::<String>() {
                    tags.push(tag.clone());
                } else {
                    return Err(TaudError::InvalidData("Invalid parameter \"tags\"".to_string()))
                }
            }

            tags
        };

        let assigns = {
            let mut assigns = vec![];

            for val in params["assign"].get::<Vec<JsonValue>>().unwrap().iter() {
                if let Some(assign) = val.get::<String>() {
                    assigns.push(assign.clone());
                } else {
                    return Err(TaudError::InvalidData("Invalid parameter \"assign\"".to_string()))
                }
            }

            assigns
        };

        let projects = {
            let mut projects = vec![];

            for val in params["project"].get::<Vec<JsonValue>>().unwrap().iter() {
                if let Some(project) = val.get::<String>() {
                    projects.push(project.clone());
                } else {
                    return Err(TaudError::InvalidData("Invalid parameter \"project\"".to_string()))
                }
            }

            projects
        };

        let created_at = match params["created_at"] {
            JsonValue::Number(numba) => Some(numba as u64),
            _ => return Err(TaudError::InvalidData("Invalid parameter \"created_at\"".to_string())),
        };

        let ws = self.workspace.lock().await.clone();
        if self.workspaces.get(&ws).unwrap().write_key.is_none() {
            info!("You don't have write access!");
            return Ok(JsonValue::Boolean(false))
        }

        let mut new_task: TaskInfo = TaskInfo::new(
            ws,
            params["title"].get::<String>().unwrap(),
            params["desc"].get::<String>().unwrap(),
            &self.nickname,
            due,
            rank,
            Timestamp::from_u64(created_at.unwrap()),
            bounty,
        )?;
        new_task.set_project(&projects);
        new_task.set_assign(&assigns);
        new_task.set_tags(&tags);

        self.notify_queue_sender.send(new_task.clone()).await.map_err(Error::from)?;
        Ok(new_task.ref_id.clone().into())
    }

    // RPCAPI:
    // List tasks
    // --> {"jsonrpc": "2.0", "method": "get_ids", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": [task_id, ...], "id": 1}
    async fn get_ref_ids(&self, params: JsonValue) -> TaudResult<JsonValue> {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        debug!(target: "tau", "JsonRpc::get_ids() params {params:?}");

        let ws = self.workspace.lock().await.clone();
        let tasks = MonthTasks::load_current_tasks(&self.dataset_path, ws, false)?;

        let task_ref_ids: Vec<JsonValue> =
            tasks.iter().map(|task| JsonValue::String(task.get_ref_id())).collect();

        Ok(JsonValue::Array(task_ref_ids))
    }

    // RPCAPI:
    // List tasks
    // --> {"jsonrpc": "2.0", "method": "get_ids", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": [task_id, ...], "id": 1}
    async fn get_archive_ref_ids(&self, params: JsonValue) -> TaudResult<JsonValue> {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        debug!(target: "tau", "JsonRpc::get_archive_ref_ids() params {params:?}");

        let month = match params[0].get::<String>() {
            Some(u64_str) => match u64_str.parse::<u64>() {
                Ok(v) => Some(Timestamp::from_u64(v)),
                //Err(e) => return Err(TaudError::InvalidData(e.to_string())),
                Err(_) => None,
            },

            None => None,
        };

        let ws = self.workspace.lock().await.clone();
        let tasks = MonthTasks::load_stop_tasks(&self.dataset_path, ws, month.as_ref())?;

        let task_ref_ids: Vec<JsonValue> =
            tasks.iter().map(|task| JsonValue::String(task.get_ref_id())).collect();

        Ok(JsonValue::Array(task_ref_ids))
    }

    // RPCAPI:
    // Modify task and returns `true` upon success.
    // --> {"jsonrpc": "2.0", "method": "modify", "params": [task_id, {"title": "new title"} ], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    async fn modify(&self, params: JsonValue) -> TaudResult<JsonValue> {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        debug!(target: "tau", "JsonRpc::modify() params {params:?}");

        if params.len() != 2 || !params[0].is_string() || !params[1].is_object() {
            return Err(TaudError::InvalidData("len of params should be 2".into()))
        }

        let ws = self.workspace.lock().await.clone();
        if self.workspaces.get(&ws).unwrap().write_key.is_none() {
            info!("You don't have write access!");
            return Ok(JsonValue::Boolean(false))
        }

        let task = self.check_params_for_modify(
            params[0].get::<String>().unwrap(),
            params[1].get::<HashMap<String, JsonValue>>().unwrap(),
            ws,
        )?;

        self.notify_queue_sender.send(task).await.map_err(Error::from)?;

        Ok(JsonValue::Boolean(true))
    }

    // RPCAPI:
    // Set state for a task and returns `true` upon success.
    // --> {"jsonrpc": "2.0", "method": "set_state", "params": [task_id, state], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    async fn set_state(&self, params: JsonValue) -> TaudResult<JsonValue> {
        // Allowed states for a task
        let states = ["stop", "start", "open", "pause"];

        let params = params.get::<Vec<JsonValue>>().unwrap();
        debug!(target: "tau", "JsonRpc::set_state() params {params:?}");

        if params.len() != 2 || !params[0].is_string() || !params[1].is_string() {
            return Err(TaudError::InvalidData("len of params should be 2".into()))
        }

        let state = params[1].get::<String>().unwrap();
        let ws = self.workspace.lock().await.clone();
        if self.workspaces.get(&ws).unwrap().write_key.is_none() {
            info!("You don't have write access!");
            return Ok(JsonValue::Boolean(false))
        }

        let mut task: TaskInfo =
            self.load_task_by_ref_id(params[0].get::<String>().unwrap(), ws)?;

        if states.contains(&state.as_str()) {
            task.set_state(state);
            set_event(&mut task, "state", &self.nickname, state);
        }

        self.notify_queue_sender.send(task).await.map_err(Error::from)?;

        Ok(JsonValue::Boolean(true))
    }

    // RPCAPI:
    // Set comment for a task and returns `true` upon success.
    // --> {"jsonrpc": "2.0", "method": "set_comment", "params": [task_id, comment_content], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    async fn set_comment(&self, params: JsonValue) -> TaudResult<JsonValue> {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        debug!(target: "tau", "JsonRpc::set_comment() params {params:?}");

        if params.len() != 2 || !params[0].is_string() || !params[1].is_string() {
            return Err(TaudError::InvalidData("len of params should be 2".into()))
        }

        let ref_id = params[0].get::<String>().unwrap();
        let comment_content = params[1].get::<String>().unwrap();

        let ws = self.workspace.lock().await.clone();
        if self.workspaces.get(&ws).unwrap().write_key.is_none() {
            info!("You don't have write access!");
            return Ok(JsonValue::Boolean(false))
        }

        let mut task: TaskInfo = self.load_task_by_ref_id(ref_id, ws)?;

        task.set_comment(Comment::new(comment_content, &self.nickname));
        set_event(&mut task, "comment", &self.nickname, comment_content);

        self.notify_queue_sender.send(task).await.map_err(Error::from)?;

        Ok(JsonValue::Boolean(true))
    }

    // RPCAPI:
    // Get a task by id.
    // --> {"jsonrpc": "2.0", "method": "get_task_by_id", "params": [task_id], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "task", "id": 1}
    async fn get_task_by_ref_id(&self, params: JsonValue) -> TaudResult<JsonValue> {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        debug!(target: "tau", "JsonRpc::get_task_by_ref_id() params {params:?}");

        if params.len() != 1 || !params[0].is_string() {
            return Err(TaudError::InvalidData("len of params should be 1".into()))
        }

        let ws = self.workspace.lock().await.clone();
        let task: TaskInfo = self.load_task_by_ref_id(params[0].get::<String>().unwrap(), ws)?;
        let task: JsonValue = (&task).into();

        Ok(task)
    }

    // RPCAPI:
    // Get all tasks.
    // --> {"jsonrpc": "2.0", "method": "fetch_deactive_tasks", "params": [task_id], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "task", "id": 1}
    async fn fetch_deactive_tasks(&self, params: JsonValue) -> TaudResult<JsonValue> {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        debug!(target: "tau", "JsonRpc::fetch_deactive_tasks() params {params:?}");

        if params.len() != 1 || !params[0].is_string() {
            return Err(TaudError::InvalidData("len of params should be 1".into()))
        }

        let month = match params[0].get::<String>() {
            Some(u64_str) => match u64_str.parse::<u64>() {
                Ok(v) => Some(Timestamp::from_u64(v)),
                //Err(e) => return Err(TaudError::InvalidData(e.to_string())),
                Err(_) => None,
            },

            None => None,
        };

        let ws = self.workspace.lock().await.clone();

        let tasks = MonthTasks::load_stop_tasks(&self.dataset_path, ws, month.as_ref())?;
        let tasks: Vec<JsonValue> = tasks.iter().map(|x| x.into()).collect();

        Ok(JsonValue::Array(tasks))
    }

    async fn fetch_archive_task(&self, params: JsonValue) -> TaudResult<JsonValue> {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        debug!(target: "tau", "JsonRpc::fetch_archive_task() params {params:?}");

        if params.len() != 2 || !params[0].is_string() || !params[1].is_string() {
            return Err(TaudError::InvalidData("len of params should be 2".into()))
        }

        let ref_id = params[0].get::<String>().unwrap();

        let month = match params[1].get::<String>() {
            Some(u64_str) => match u64_str.parse::<u64>() {
                Ok(v) => Some(Timestamp::from_u64(v)),
                //Err(e) => return Err(TaudError::InvalidData(e.to_string())),
                Err(_) => None,
            },

            None => None,
        };

        let ws = self.workspace.lock().await.clone();

        let mut tasks = MonthTasks::load_stop_tasks(&self.dataset_path, ws, month.as_ref())?;
        tasks.retain(|x| x.ref_id == *ref_id);

        if tasks.len() != 1 {
            return Err(TaudError::InvalidData("Must return a single value".into()))
        }

        let task: JsonValue = (&tasks[0]).into();

        Ok(task)
    }

    // RPCAPI:
    // Switch tasks workspace.
    // --> {"jsonrpc": "2.0", "method": "switch_ws", "params": [workspace], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "true", "id": 1}
    async fn switch_ws(&self, params: JsonValue) -> TaudResult<JsonValue> {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        debug!(target: "tau", "JsonRpc::switch_ws() params {params:?}");

        if params.len() != 1 {
            return Err(TaudError::InvalidData("len of params should be 1".into()))
        }

        if !params[0].is_string() {
            return Err(TaudError::InvalidData("Invalid workspace".into()))
        }

        let ws = params[0].get::<String>().unwrap();
        let mut s = self.workspace.lock().await;

        if self.workspaces.contains_key(ws) {
            *s = ws.to_string()
        } else {
            warn!("Workspace \"{ws}\" is not configured");
            return Ok(JsonValue::Boolean(false))
        }

        Ok(JsonValue::Boolean(true))
    }

    // RPCAPI:
    // Get workspace.
    // --> {"jsonrpc": "2.0", "method": "get_ws", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "workspace", "id": 1}
    async fn get_ws(&self, params: JsonValue) -> TaudResult<JsonValue> {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        debug!(target: "tau", "JsonRpc::get_ws() params {params:?}");
        let ws = self.workspace.lock().await.clone();
        Ok(JsonValue::String(ws))
    }

    // RPCAPI:
    // Export tasks.
    // --> {"jsonrpc": "2.0", "method": "export_to", "params": [path], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "true", "id": 1}
    async fn export_to(&self, params: JsonValue) -> TaudResult<JsonValue> {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        debug!(target: "tau", "JsonRpc::export_to() params {params:?}");

        if params.len() != 1 {
            return Err(TaudError::InvalidData("len of params should be 1".into()))
        }

        if !params[0].is_string() {
            return Err(TaudError::InvalidData("Invalid path".into()))
        }

        // mkdir datastore_path if not exists
        let path = params[0].get::<String>().unwrap();
        let path = expand_path(path)?.join("exported_tasks");
        create_dir_all(path.join("month")).map_err(Error::from)?;
        create_dir_all(path.join("task")).map_err(Error::from)?;

        let ws = self.workspace.lock().await.clone();
        let tasks = MonthTasks::load_current_tasks(&self.dataset_path, ws, true)?;

        for task in tasks {
            task.save(&path)?;
        }

        Ok(JsonValue::Boolean(true))
    }

    // RPCAPI:
    // Import tasks.
    // --> {"jsonrpc": "2.0", "method": "import_from", "params": [path], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "true", "id": 1}
    async fn import_from(&self, params: JsonValue) -> TaudResult<JsonValue> {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        debug!(target: "tau", "JsonRpc::import_from() params {params:?}");

        if params.len() != 1 {
            return Err(TaudError::InvalidData("len of params should be 1".into()))
        }

        if !params[0].is_string() {
            return Err(TaudError::InvalidData("Invalid path".into()))
        }

        let path = params[0].get::<String>().unwrap();
        let path = expand_path(path)?.join("exported_tasks");
        let ws = self.workspace.lock().await.clone();
        if self.workspaces.get(&ws).unwrap().write_key.is_none() {
            info!("You don't have write access!");
            return Ok(JsonValue::Boolean(false))
        }

        let imported_tasks = MonthTasks::load_current_tasks(&path, ws.clone(), true)?;

        for task in imported_tasks {
            if MonthTasks::load_current_tasks(&self.dataset_path, ws.clone(), false)?
                .into_iter()
                .map(|t| t.ref_id)
                .any(|x| x == task.ref_id)
            {
                continue
            }

            self.notify_queue_sender.send(task).await.map_err(Error::from)?;
        }
        Ok(JsonValue::Boolean(true))
    }

    fn load_task_by_ref_id(&self, task_ref_id: &str, ws: String) -> TaudResult<TaskInfo> {
        let tasks = MonthTasks::load_current_tasks(&self.dataset_path, ws, false)?;
        let task = tasks.into_iter().find(|t| (t.get_ref_id()) == task_ref_id);

        task.ok_or(TaudError::InvalidId)
    }

    fn check_params_for_modify(
        &self,
        task_ref_id: &str,
        fields: &HashMap<String, JsonValue>,
        ws: String,
    ) -> TaudResult<TaskInfo> {
        let mut task: TaskInfo = self.load_task_by_ref_id(task_ref_id, ws)?;

        if fields.contains_key("title") {
            let title = fields["title"].get::<String>().unwrap();
            if !title.is_empty() {
                task.set_title(title);
                set_event(&mut task, "title", &self.nickname, title);
            }
        }

        if fields.contains_key("desc") {
            let desc = fields["desc"].get::<String>().unwrap();
            if !desc.is_empty() {
                task.set_desc(desc);
                set_event(&mut task, "desc", &self.nickname, desc);
            }
        }

        if fields.contains_key("rank") {
            match fields["rank"] {
                JsonValue::Null => set_event(&mut task, "rank", &self.nickname, "None"),
                JsonValue::Number(rank) => {
                    task.set_rank(Some(rank as f32));
                    set_event(&mut task, "rank", &self.nickname, &rank.to_string())
                }
                _ => unreachable!(),
            }
        }

        if fields.contains_key("bounty") {
            match fields["bounty"] {
                JsonValue::Null => set_event(&mut task, "bounty", &self.nickname, "None"),
                JsonValue::Number(bounty) => {
                    task.set_bounty(Some(bounty as f32));
                    set_event(&mut task, "bounty", &self.nickname, &bounty.to_string())
                }
                _ => unreachable!(),
            }
        }

        if fields.contains_key("due") {
            match &fields["due"] {
                JsonValue::Null => set_event(&mut task, "due", &self.nickname, "None"),
                JsonValue::Number(ts_num) => {
                    task.set_due(Some(Timestamp::from_u64(*ts_num as u64)));
                    set_event(&mut task, "due", &self.nickname, &ts_num.to_string())
                }
                _ => unreachable!(),
            }
        }

        if fields.contains_key("assign") {
            let assign: Vec<String> = fields["assign"]
                .get::<Vec<JsonValue>>()
                .unwrap()
                .iter()
                .map(|x| x.get::<String>().unwrap().clone())
                .collect();

            if !assign.is_empty() {
                task.set_assign(&assign);
                set_event(&mut task, "assign", &self.nickname, &assign.join(", "));
            }
        }

        if fields.contains_key("project") {
            let project: Vec<String> = fields["project"]
                .get::<Vec<JsonValue>>()
                .unwrap()
                .iter()
                .map(|x| x.get::<String>().unwrap().clone())
                .collect();

            if !project.is_empty() {
                task.set_project(&project);
                set_event(&mut task, "project", &self.nickname, &project.join(", "));
            }
        }

        if fields.contains_key("tags") {
            let tags: Vec<String> = fields["tags"]
                .get::<Vec<JsonValue>>()
                .unwrap()
                .iter()
                .map(|x| x.get::<String>().unwrap().clone())
                .collect();

            if !tags.is_empty() {
                task.set_tags(&tags);
                set_event(&mut task, "tags", &self.nickname, &tags.join(", "));
            }
        }

        Ok(task)
    }

    // RPCAPI:
    // Register a pregenerated RLN identity under a local account name.
    // Pregenerated identities are already bootstrapped into the static
    // DAG; this command does not broadcast a public free-tier
    // registration proof. The first account registered also becomes the
    // active one.
    //
    // --> {"jsonrpc": "2.0", "method": "rln_register",
    //      "params": [account_name, nullifier, trapdoor, user_msg_limit], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": ["...", ...], "id": 1}
    async fn rln_register(&self, params: JsonValue) -> TaudResult<JsonValue> {
        if !self.event_graph.rln_enabled() {
            return Ok(strings_to_json(vec![
                "RLN is disabled; registration is not required.".to_string()
            ]))
        }

        let params = params.get::<Vec<JsonValue>>().unwrap();
        debug!(target: "tau", "JsonRpc::rln_register() params {params:?}");

        if params.len() != 4 ||
            !params[0].is_string() ||
            !params[1].is_string() ||
            !params[2].is_string() ||
            !params[3].is_string()
        {
            return Err(TaudError::InvalidData(
                "len of params should be 4 (account_name, nullifier, trapdoor, user_msg_limit)"
                    .into(),
            ))
        }

        let account_name = params[0].get::<String>().unwrap();
        let identity_nullifier = params[1].get::<String>().unwrap();
        let identity_trapdoor = params[2].get::<String>().unwrap();
        let user_msg_limit_str = params[3].get::<String>().unwrap();

        // Reserved name. We use `default` for the mirror tree.
        if !is_valid_account_name(account_name) {
            return Ok(strings_to_json(vec!["Invalid account name.".to_string()]))
        }

        // Parse user_msg_limit defensively so a typo doesn't tear the
        // daemon down.
        let user_msg_limit: u64 = match user_msg_limit_str.parse() {
            Ok(v) => v,
            Err(_) => {
                return Ok(strings_to_json(vec![
                    "Invalid user_msg_limit: must be a positive integer.".to_string(),
                ]))
            }
        };
        if user_msg_limit == 0 {
            return Ok(strings_to_json(vec![
                "Invalid user_msg_limit: must be at least 1.".to_string()
            ]))
        }

        // Parse the secrets, gracefully rejecting malformed base58.
        let identity_nullifier = match parse_pallas_b58(identity_nullifier) {
            Some(v) => v,
            None => return Ok(strings_to_json(vec!["Invalid identity_nullifier.".to_string()])),
        };
        let identity_trapdoor = match parse_pallas_b58(identity_trapdoor) {
            Some(v) => v,
            None => return Ok(strings_to_json(vec!["Invalid identity_trapdoor.".to_string()])),
        };

        // `last_epoch` is initialised to 0 deterministically - the first
        // persisted send reservation will detect the rollover to the
        // current wall-clock epoch.
        let new_rln_identity = RlnIdentity {
            nullifier: identity_nullifier,
            trapdoor: identity_trapdoor,
            user_message_limit: user_msg_limit,
            message_id: 0,
            last_epoch: 0,
        };

        if !is_pregenerated_commitment(&new_rln_identity.commitment()) {
            return Ok(strings_to_json(vec![
                "Registration is currently limited to pregenerated identities.".to_string(),
            ]))
        }

        if user_msg_limit != GENESIS_USER_MSG_LIMIT {
            return Ok(strings_to_json(vec![format!(
                "Genesis account must use user_msg_limit={}",
                GENESIS_USER_MSG_LIMIT
            )]))
        }

        // Open the per-account sled tree only after the identity has
        // passed the pregenerated-admission checks.
        let db = self.sled_db.open_tree(format!("{ACCOUNTS_DB_PREFIX}{account_name}"))?;
        if !db.is_empty() {
            return Ok(strings_to_json(vec!["This account name is already registered.".to_string()]))
        }

        // Store account.
        db.insert(ACCOUNTS_KEY_RLN_IDENTITY, serialize_async(&new_rln_identity).await)?;

        // First-ever registration also becomes the active one.
        let became_active = self.rln_identity.read().await.is_none();
        if became_active {
            let db_default = self.sled_db.open_tree(ACCOUNTS_DEFAULT_TREE)?;
            db_default
                .insert(ACCOUNTS_KEY_RLN_IDENTITY, serialize_async(&new_rln_identity).await)?;
            *self.rln_identity.write().await = Some(new_rln_identity);
        }

        let mut replies = vec![format!("Successfully registered account \"{account_name}\"")];
        if became_active {
            replies.push(format!("\"{account_name}\" is now the active identity."));
        } else {
            replies.push(format!("Use `rln_set {account_name}` to make this the active identity."));
        }

        Ok(strings_to_json(replies))
    }

    // RPCAPI:
    // List registered accounts (with active marker), or dump the
    // secrets for a single account.
    //
    // --> {"jsonrpc": "2.0", "method": "rln_info", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": ["...", ...], "id": 1}
    // --> {"jsonrpc": "2.0", "method": "rln_info", "params": [account_name], "id": 1}
    async fn rln_info(&self, params: JsonValue) -> TaudResult<JsonValue> {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        debug!(target: "tau", "JsonRpc::rln_info() params {params:?}");

        if params.len() > 1 {
            return Err(TaudError::InvalidData("rln_info takes at most one account_name".into()))
        }

        let account_name = params.first().and_then(|p| p.get::<String>()).cloned();
        if let Some(account_name) = account_name {
            return Ok(strings_to_json(self.rln_info_account(&account_name).await?))
        }

        // The active identity's commitment is what we compare against.
        let active_commitment = self.rln_identity.read().await.as_ref().map(|id| id.commitment());

        let mut accounts: Vec<(String, RlnIdentity)> = Vec::new();
        for raw in self.sled_db.tree_names() {
            let bytes: &[u8] = raw.as_ref();
            let Ok(name) = std::str::from_utf8(bytes) else { continue };
            // Skip the `default` mirror tree and anything that isn't an
            // account tree.
            let Some(account_name) = name.strip_prefix(ACCOUNTS_DB_PREFIX) else { continue };
            if !is_valid_account_name(account_name) {
                continue
            }

            let tree = self.sled_db.open_tree(name)?;
            let Some(blob) = tree.get(ACCOUNTS_KEY_RLN_IDENTITY)? else { continue };
            let Ok(identity): std::result::Result<RlnIdentity, _> = deserialize_async(&blob).await
            else {
                continue
            };

            accounts.push((account_name.to_string(), identity));
        }

        if accounts.is_empty() {
            return Ok(strings_to_json(vec![
                "No registered accounts. Use rln_register to create one.".to_string(),
            ]))
        }

        accounts.sort_by(|a, b| a.0.cmp(&b.0));

        let mut lines = vec!["Registered accounts (* = active):".to_string()];
        for (name, id) in &accounts {
            let active_mark = if Some(id.commitment()) == active_commitment { "*" } else { " " };
            let commitment_b58 = bs58::encode(id.commitment().to_repr()).into_string();
            lines.push(format!(
                "  {active_mark} {name}  limit={}  commitment={commitment_b58}",
                id.user_message_limit,
            ));
        }
        lines.push(
            "Use `rln_info <account_name>` to show that account's secrets (rln_register args)."
                .to_string(),
        );

        Ok(strings_to_json(lines))
    }

    /// `rln_info <account_name>`. Dumps the secrets so the user can
    /// reconstruct the identity elsewhere.
    async fn rln_info_account(&self, account_name: &str) -> TaudResult<Vec<String>> {
        if !is_valid_account_name(account_name) {
            return Ok(vec!["Invalid account name.".to_string()])
        }

        let tree_name = format!("{ACCOUNTS_DB_PREFIX}{account_name}");
        let tree = self.sled_db.open_tree(&tree_name)?;
        let Some(blob) = tree.get(ACCOUNTS_KEY_RLN_IDENTITY)? else {
            return Ok(vec![format!("No such account: \"{account_name}\"")])
        };
        let identity: RlnIdentity = match deserialize_async(&blob).await {
            Ok(v) => v,
            Err(_) => {
                return Ok(vec![format!(
                    "Account \"{account_name}\" exists but its data is corrupted."
                )])
            }
        };

        let nullifier_b58 = bs58::encode(identity.nullifier.to_repr()).into_string();
        let trapdoor_b58 = bs58::encode(identity.trapdoor.to_repr()).into_string();
        let commitment_b58 = bs58::encode(identity.commitment().to_repr()).into_string();

        let active_commitment = self.rln_identity.read().await.as_ref().map(|id| id.commitment());
        let is_active = Some(identity.commitment()) == active_commitment;

        let mut lines = vec![format!(
            "Account \"{account_name}\"{}:",
            if is_active { " (ACTIVE)" } else { "" },
        )];
        lines.push(format!("  commitment       = {commitment_b58}"));
        lines.push(format!("  user_msg_limit   = {}", identity.user_message_limit));
        lines.push("  --- secrets below; treat as a password ---".to_string());
        lines.push(format!("  nullifier        = {nullifier_b58}"));
        lines.push(format!("  trapdoor         = {trapdoor_b58}"));
        lines.push("To re-register on another node, run:".to_string());
        lines.push(format!(
            "  tau rln register {account_name} {nullifier_b58} {trapdoor_b58} {limit}",
            limit = identity.user_message_limit,
        ));

        Ok(lines)
    }

    // RPCAPI:
    // Swap the active identity to the named account. The choice is
    // persisted (next restart will load the same one) and takes effect
    // for the next outbound task event.
    //
    // --> {"jsonrpc": "2.0", "method": "rln_set", "params": [account_name], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": ["...", ...], "id": 1}
    async fn rln_set(&self, params: JsonValue) -> TaudResult<JsonValue> {
        if !self.event_graph.rln_enabled() {
            return Ok(strings_to_json(vec!["RLN is disabled; rln_set has no effect.".to_string()]))
        }

        let params = params.get::<Vec<JsonValue>>().unwrap();
        debug!(target: "tau", "JsonRpc::rln_set() params {params:?}");

        if params.len() != 1 || !params[0].is_string() {
            return Err(TaudError::InvalidData("len of params should be 1".into()))
        }

        let account_name = params[0].get::<String>().unwrap();
        if !is_valid_account_name(account_name) {
            return Ok(strings_to_json(vec!["Invalid account name.".to_string()]))
        }

        let tree_name = format!("{ACCOUNTS_DB_PREFIX}{account_name}");
        let tree = self.sled_db.open_tree(&tree_name)?;
        let Some(blob) = tree.get(ACCOUNTS_KEY_RLN_IDENTITY)? else {
            return Ok(strings_to_json(vec![
                format!("No such account: \"{account_name}\""),
                "Use rln_info to list registered accounts.".to_string(),
            ]))
        };
        let identity: RlnIdentity = match deserialize_async(&blob).await {
            Ok(v) => v,
            Err(_) => {
                return Ok(strings_to_json(vec![format!(
                    "Account \"{account_name}\" data is corrupted."
                )]))
            }
        };

        // No-op if it's already active.
        let already_active = match self.rln_identity.read().await.as_ref() {
            Some(active) => active.commitment() == identity.commitment(),
            None => false,
        };
        if already_active {
            return Ok(strings_to_json(vec![format!(
                "\"{account_name}\" is already the active identity."
            )]))
        }

        // Persist the choice. We write the freshly-loaded blob (not the
        // in-memory identity, which would have stale counter state if it
        // were the previously-active one) because the default tree is
        // meant to mirror an account tree exactly.
        let db_default = self.sled_db.open_tree(ACCOUNTS_DEFAULT_TREE)?;
        db_default.insert(ACCOUNTS_KEY_RLN_IDENTITY, blob.as_ref())?;

        *self.rln_identity.write().await = Some(identity);

        Ok(strings_to_json(vec![
            format!("Active identity is now \"{account_name}\"."),
            "If you have used this identity recently from another node, wait one RLN epoch \
             (10 minutes) before sending to avoid a counter clash."
                .to_string(),
        ]))
    }

    // RPCAPI:
    // Remove an account from local storage. The on-network RLN
    // registration is permanent; this only forgets the account locally.
    // Refuses to drop the active account.
    //
    // --> {"jsonrpc": "2.0", "method": "rln_deregister", "params": [account_name], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": ["...", ...], "id": 1}
    async fn rln_deregister(&self, params: JsonValue) -> TaudResult<JsonValue> {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        debug!(target: "tau", "JsonRpc::rln_deregister() params {params:?}");

        if params.len() != 1 || !params[0].is_string() {
            return Err(TaudError::InvalidData("len of params should be 1".into()))
        }

        let account_name = params[0].get::<String>().unwrap();
        if !is_valid_account_name(account_name) {
            return Ok(strings_to_json(vec!["Invalid account name.".to_string()]))
        }

        let tree_name = format!("{ACCOUNTS_DB_PREFIX}{account_name}");
        let tree = self.sled_db.open_tree(&tree_name)?;
        let Some(blob) = tree.get(ACCOUNTS_KEY_RLN_IDENTITY)? else {
            return Ok(strings_to_json(vec![format!("No such account: \"{account_name}\"")]))
        };
        let identity: RlnIdentity = match deserialize_async(&blob).await {
            Ok(v) => v,
            Err(_) => {
                // Corrupted account: allow the user to reclaim the tree
                // name, but err on the safe side if there IS an active one.
                if self.rln_identity.read().await.is_some() {
                    return Ok(strings_to_json(vec![format!(
                        "Account \"{account_name}\" data is corrupted; refusing to \
                         auto-deregister while another identity is active. rln_set to \
                         a clean account first, then retry."
                    )]))
                }
                self.sled_db.drop_tree(&tree_name)?;
                return Ok(strings_to_json(vec![format!(
                    "Dropped corrupted account \"{account_name}\"."
                )]))
            }
        };

        // Refuse if active.
        if let Some(active) = self.rln_identity.read().await.as_ref() {
            if active.commitment() == identity.commitment() {
                return Ok(strings_to_json(vec![
                    format!("\"{account_name}\" is the active identity; refusing to deregister."),
                    "Use `rln_set <other_account>` first to switch away.".to_string(),
                ]))
            }
        }

        self.sled_db.drop_tree(&tree_name)?;

        Ok(strings_to_json(vec![format!("Successfully deregistered account \"{account_name}\"")]))
    }

    // RPCAPI:
    // Permanently retire an account on the network. Publishes a slash
    // event into the static DAG; once accepted by peers the identity is
    // removed from the SMT network-wide and CANNOT be re-registered. The
    // slash blob contains the identity_secret_hash in plaintext, so the
    // secret becomes world-readable on the wire.
    //
    // --> {"jsonrpc": "2.0", "method": "rln_slash", "params": [account_name], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": ["...", ...], "id": 1}
    async fn rln_slash(&self, params: JsonValue) -> TaudResult<JsonValue> {
        if !self.event_graph.rln_enabled() {
            return Ok(strings_to_json(vec![
                "RLN is disabled; rln_slash is unavailable.".to_string()
            ]))
        }

        let params = params.get::<Vec<JsonValue>>().unwrap();
        debug!(target: "tau", "JsonRpc::rln_slash() params {params:?}");

        if params.len() != 1 || !params[0].is_string() {
            return Err(TaudError::InvalidData("len of params should be 1".into()))
        }

        let account_name = params[0].get::<String>().unwrap();
        if !is_valid_account_name(account_name) {
            return Ok(strings_to_json(vec!["Invalid account name.".to_string()]))
        }

        let tree_name = format!("{ACCOUNTS_DB_PREFIX}{account_name}");
        let tree = self.sled_db.open_tree(&tree_name)?;
        let Some(blob) = tree.get(ACCOUNTS_KEY_RLN_IDENTITY)? else {
            return Ok(strings_to_json(vec![format!("No such account: \"{account_name}\"")]))
        };
        let identity: RlnIdentity = match deserialize_async(&blob).await {
            Ok(v) => v,
            Err(_) => {
                return Ok(strings_to_json(vec![format!(
                    "Account \"{account_name}\" data is corrupted."
                )]))
            }
        };

        // Refuse if active.
        if let Some(active) = self.rln_identity.read().await.as_ref() {
            if active.commitment() == identity.commitment() {
                return Ok(strings_to_json(vec![
                    format!("\"{account_name}\" is the active identity; refusing to slash."),
                    "Use `rln_set <other_account>` first if you genuinely want to slash \
                     this identity."
                        .to_string(),
                ]))
            }
        }

        // Refuse while unsynced. The slash proof's public input includes
        // the current SMT root, which peers verify against their own
        // historical-roots table.
        let evgr = &self.event_graph;
        if !evgr.is_synced() {
            return Ok(strings_to_json(vec![
                "Cannot rln_slash while the local DAG is unsynced.".to_string(),
                "Wait for sync to complete and try again.".to_string(),
            ]))
        }

        // Build the slash proof. The request contains identity_secret_hash
        // (NOT the raw nullifier+trapdoor pair) because that's what SSS
        // would recover in the misbehavior path.
        let identity_secret_hash = identity.identity_secret_hash();
        let request = {
            let id_state = evgr.rln_identity_state()?.read().await;
            prepare_slash_proof_request(identity_secret_hash, &id_state)
        };
        let root = request.merkle_root;

        log_memory("before slash proving");
        let proof = evgr.rln_zk_keys()?.prove_slash(request).await?.proof;
        log_memory("after slash proving");

        let slash_blob = SlashBlob { proof, identity_secret_hash, merkle_root: root };
        let blob_bytes = serialize_async(&slash_blob).await;

        let rln_node = RLNNode::Slashing(identity.commitment());
        let event = Event::new_static(serialize_async(&rln_node).await, evgr).await?;

        // Commit through the verified static-event pipeline so durable event
        // storage stays ahead of RLN side tables, while subscribers still see
        // the event only after the local RLN state has been updated.
        evgr.commit_verified_static_event(&event, &blob_bytes, &rln_node).await?;
        evgr.static_broadcast(event, blob_bytes).await?;

        // Drop the local account tree. The on-network slash makes the
        // account unusable anyway.
        self.sled_db.drop_tree(&tree_name)?;

        Ok(strings_to_json(vec![
            format!("SLASHED \"{account_name}\". The identity is permanently retired."),
            "The slash event has been broadcast to peers; once propagated, the \
             commitment is removed from the network's identity tree."
                .to_string(),
            "Local account state has also been dropped.".to_string(),
        ]))
    }
}

fn strings_to_json(lines: Vec<String>) -> JsonValue {
    JsonValue::Array(lines.into_iter().map(JsonValue::String).collect())
}

fn is_account_name_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')
}

/// Return true when a local account name is safe as a sled tree suffix.
fn is_valid_account_name(account_name: &str) -> bool {
    account_name != "default" &&
        !account_name.is_empty() &&
        account_name.len() <= MAX_ACCOUNT_NAME_LEN &&
        account_name.bytes().all(is_account_name_char)
}

/// Decode a base58-encoded `pallas::Base` scalar. Returns `None` for
/// any malformed input rather than panicking - this is called on
/// user-supplied RPC parameters.
fn parse_pallas_b58(s: &str) -> Option<pallas::Base> {
    let bytes = bs58::decode(s).into_vec().ok()?;
    let arr: [u8; 32] = bytes.try_into().ok()?;
    pallas::Base::from_repr(arr).into_option()
}
