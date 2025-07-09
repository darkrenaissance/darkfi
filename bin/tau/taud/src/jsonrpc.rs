/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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
    collections::{HashMap, HashSet},
    fs::create_dir_all,
    path::PathBuf,
    sync::Arc,
};

use async_trait::async_trait;
use smol::lock::{Mutex, MutexGuard};
use tinyjson::JsonValue;
use tracing::{debug, info, warn};

use darkfi::{
    event_graph::EventGraphPtr,
    net,
    rpc::{
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResult, JsonSubscriber},
        p2p_method::HandlerP2p,
        server::RequestHandler,
    },
    system::StoppableTaskPtr,
    util::{path::expand_path, time::Timestamp},
    Error,
};

use taud::{
    error::{to_json_result, TaudError, TaudResult},
    month_tasks::MonthTasks,
    task_info::{Comment, TaskInfo},
    util::set_event,
};

use crate::Workspace;

const DEFAULT_WORKSPACE: &str = "darkfi-dev";

pub struct JsonRpcInterface {
    dataset_path: PathBuf,
    notify_queue_sender: smol::channel::Sender<TaskInfo>,
    nickname: String,
    workspace: Mutex<String>,
    workspaces: Arc<HashMap<String, Workspace>>,
    p2p: net::P2pPtr,
    event_graph: EventGraphPtr,
    dnet_sub: JsonSubscriber,
    deg_sub: JsonSubscriber,
    rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
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
        workspaces: Arc<HashMap<String, Workspace>>,
        p2p: net::P2pPtr,
        event_graph: EventGraphPtr,
        dnet_sub: JsonSubscriber,
        deg_sub: JsonSubscriber,
    ) -> Self {
        let workspace = Mutex::new(DEFAULT_WORKSPACE.to_string());
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
    pub async fn dnet_subscribe_events(&self, id: u16, params: JsonValue) -> JsonResult {
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
    pub async fn deg_subscribe_events(&self, id: u16, params: JsonValue) -> JsonResult {
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
    async fn deg_switch(&self, _id: u16, params: JsonValue) -> TaudResult<JsonValue> {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 1 || !params[0].is_bool() {
            return Err(TaudError::InvalidData("Invalid parameters".into()))
        }

        let switch = params[0].get::<bool>().unwrap();

        if *switch {
            self.event_graph.deg_enable().await;
        } else {
            self.event_graph.deg_disable().await;
        }

        Ok(JsonValue::Boolean(true))
    }

    // RPCAPI:
    // Get EVENTGRAPH info.
    //
    // --> {"jsonrpc": "2.0", "method": "deg.switch", "params": [true], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 42}
    async fn eg_get_info(&self, id: u16, params: JsonValue) -> JsonResult {
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

        if params.len() != 9 {
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
}
