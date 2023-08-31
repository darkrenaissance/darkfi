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

use std::{
    collections::{HashMap, HashSet},
    fs::create_dir_all,
    path::PathBuf,
    sync::Arc,
};

use async_trait::async_trait;
use crypto_box::ChaChaBox;
use log::{debug, warn};
use smol::lock::{Mutex, MutexGuard};
use tinyjson::JsonValue;

use darkfi::{
    net,
    rpc::{
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResult},
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
    util::{find_free_id, set_event},
};

pub struct JsonRpcInterface {
    dataset_path: PathBuf,
    notify_queue_sender: smol::channel::Sender<TaskInfo>,
    nickname: String,
    workspace: Mutex<String>,
    workspaces: Arc<HashMap<String, ChaChaBox>>,
    p2p: net::P2pPtr,
    rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
}

#[async_trait]
impl RequestHandler for JsonRpcInterface {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        let rep = match req.method.as_str() {
            "add" => self.add(req.params).await,
            "get_ids" => self.get_ids(req.params).await,
            "update" => self.update(req.params).await,
            "set_state" => self.set_state(req.params).await,
            "set_comment" => self.set_comment(req.params).await,
            "get_task_by_id" => self.get_task_by_id(req.params).await,
            "switch_ws" => self.switch_ws(req.params).await,
            "get_ws" => self.get_ws(req.params).await,
            "export" => self.export_to(req.params).await,
            "import" => self.import_from(req.params).await,
            "get_stop_tasks" => self.get_stop_tasks(req.params).await,

            "ping" => return self.pong(req.id, req.params).await,
            "dnet_switch" => self.dnet_switch(req.params).await,
            _ => return JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
        };

        to_json_result(rep, req.id)
    }

    async fn connections_mut(&self) -> MutexGuard<'_, HashSet<StoppableTaskPtr>> {
        self.rpc_connections.lock().await
    }
}

impl JsonRpcInterface {
    pub fn new(
        dataset_path: PathBuf,
        notify_queue_sender: smol::channel::Sender<TaskInfo>,
        nickname: String,
        workspaces: Arc<HashMap<String, ChaChaBox>>,
        p2p: net::P2pPtr,
    ) -> Self {
        let workspace = Mutex::new(workspaces.iter().last().unwrap().0.clone());
        Self {
            dataset_path,
            nickname,
            workspace,
            workspaces,
            notify_queue_sender,
            p2p,
            rpc_connections: Mutex::new(HashSet::new()),
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
            self.p2p.dnet_enable().await;
        } else {
            self.p2p.dnet_disable().await;
        }

        Ok(JsonValue::Boolean(true))
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
        debug!(target: "tau", "JsonRpc::add() params {:?}", params);

        if params.len() != 7 ||
            !params[0].is_string() ||
            !params[1].is_array() ||
            !params[2].is_string() ||
            !params[3].is_array() ||
            !params[4].is_array()
        {
            return Err(TaudError::InvalidData("Invalid parameters".to_string()))
        }

        let due = match &params[5] {
            JsonValue::Null => None,
            JsonValue::String(u64_str) => match u64_str.parse::<u64>() {
                Ok(v) => Some(Timestamp(v)),
                Err(e) => return Err(TaudError::InvalidData(e.to_string())),
            },
            _ => return Err(TaudError::InvalidData("Invalid parameters".to_string())),
        };

        let rank = match params[6] {
            JsonValue::Null => None,
            JsonValue::Number(numba) => Some(numba as f32),
            _ => return Err(TaudError::InvalidData("Invalid parameters".to_string())),
        };

        let tags = {
            let mut tags = vec![];

            for val in params[1].get::<Vec<JsonValue>>().unwrap().iter() {
                if let Some(tag) = val.get::<String>() {
                    tags.push(tag.clone());
                } else {
                    return Err(TaudError::InvalidData("Invalid parameters".to_string()))
                }
            }

            tags
        };

        let assigns = {
            let mut assigns = vec![];

            for val in params[3].get::<Vec<JsonValue>>().unwrap().iter() {
                if let Some(assign) = val.get::<String>() {
                    assigns.push(assign.clone());
                } else {
                    return Err(TaudError::InvalidData("Invalid parameters".to_string()))
                }
            }

            assigns
        };

        let projects = {
            let mut projects = vec![];

            for val in params[4].get::<Vec<JsonValue>>().unwrap().iter() {
                if let Some(project) = val.get::<String>() {
                    projects.push(project.clone());
                } else {
                    return Err(TaudError::InvalidData("Invalid parameters".to_string()))
                }
            }

            projects
        };

        let mut new_task: TaskInfo = TaskInfo::new(
            self.workspace.lock().await.clone(),
            params[0].get::<String>().unwrap(),
            params[2].get::<String>().unwrap(),
            &self.nickname,
            due,
            rank,
            &self.dataset_path,
        )?;
        new_task.set_project(&projects);
        new_task.set_assign(&assigns);
        new_task.set_tags(&tags);

        self.notify_queue_sender.send(new_task.clone()).await.map_err(Error::from)?;
        Ok(JsonValue::Number(new_task.id.into()))
    }

    // RPCAPI:
    // List tasks
    // --> {"jsonrpc": "2.0", "method": "get_ids", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": [task_id, ...], "id": 1}
    async fn get_ids(&self, params: JsonValue) -> TaudResult<JsonValue> {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        debug!(target: "tau", "JsonRpc::get_ids() params {:?}", params);

        let ws = self.workspace.lock().await.clone();
        let tasks = MonthTasks::load_current_tasks(&self.dataset_path, ws, false)?;

        let task_ids: Vec<JsonValue> =
            tasks.iter().map(|task| JsonValue::Number(task.get_id().into())).collect();

        Ok(JsonValue::Array(task_ids))
    }

    // RPCAPI:
    // Update task and returns `true` upon success.
    // --> {"jsonrpc": "2.0", "method": "update", "params": [task_id, {"title": "new title"} ], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    async fn update(&self, params: JsonValue) -> TaudResult<JsonValue> {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        debug!(target: "tau", "JsonRpc::update() params {:?}", params);

        if params.len() != 2 || !params[0].is_number() || !params[1].is_object() {
            return Err(TaudError::InvalidData("len of params should be 2".into()))
        }

        let ws = self.workspace.lock().await.clone();

        let task = self.check_params_for_update(
            *params[0].get::<f64>().unwrap() as u32,
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
        debug!(target: "tau", "JsonRpc::set_state() params {:?}", params);

        if params.len() != 2 || !params[0].is_number() || !params[1].is_string() {
            return Err(TaudError::InvalidData("len of params should be 2".into()))
        }

        let state = params[1].get::<String>().unwrap();
        let ws = self.workspace.lock().await.clone();

        let mut task: TaskInfo =
            self.load_task_by_id(*params[0].get::<f64>().unwrap() as u32, ws)?;

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
        debug!(target: "tau", "JsonRpc::set_comment() params {:?}", params);

        if params.len() != 2 || !params[0].is_number() || !params[1].is_string() {
            return Err(TaudError::InvalidData("len of params should be 2".into()))
        }

        let id = *params[0].get::<f64>().unwrap() as u32;
        let comment_content = params[1].get::<String>().unwrap();

        let ws = self.workspace.lock().await.clone();
        let mut task: TaskInfo = self.load_task_by_id(id, ws)?;

        task.set_comment(Comment::new(comment_content, &self.nickname));
        set_event(&mut task, "comment", &self.nickname, comment_content);

        self.notify_queue_sender.send(task).await.map_err(Error::from)?;

        Ok(JsonValue::Boolean(true))
    }

    // RPCAPI:
    // Get a task by id.
    // --> {"jsonrpc": "2.0", "method": "get_task_by_id", "params": [task_id], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "task", "id": 1}
    async fn get_task_by_id(&self, params: JsonValue) -> TaudResult<JsonValue> {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        debug!(target: "tau", "JsonRpc::get_task_by_id() params {:?}", params);

        if params.len() != 1 || !params[0].is_number() {
            return Err(TaudError::InvalidData("len of params should be 1".into()))
        }

        let ws = self.workspace.lock().await.clone();
        let task: TaskInfo = self.load_task_by_id(*params[0].get::<f64>().unwrap() as u32, ws)?;
        let task: JsonValue = (&task).into();

        Ok(task)
    }

    // RPCAPI:
    // Get all tasks.
    // --> {"jsonrpc": "2.0", "method": "get_stop_tasks", "params": [task_id], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "task", "id": 1}
    async fn get_stop_tasks(&self, params: JsonValue) -> TaudResult<JsonValue> {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        debug!(target: "tau", "JsonRpc::get_stop_tasks() params {:?}", params);

        if params.len() != 1 || !params[0].is_string() {
            return Err(TaudError::InvalidData("len of params should be 1".into()))
        }

        let month = match params[0].get::<String>() {
            Some(u64_str) => match u64_str.parse::<u64>() {
                Ok(v) => Some(Timestamp(v)),
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

    // RPCAPI:
    // Switch tasks workspace.
    // --> {"jsonrpc": "2.0", "method": "switch_ws", "params": [workspace], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "true", "id": 1}
    async fn switch_ws(&self, params: JsonValue) -> TaudResult<JsonValue> {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        debug!(target: "tau", "JsonRpc::switch_ws() params {:?}", params);

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
            warn!("Workspace \"{}\" is not configured", ws);
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
        debug!(target: "tau", "JsonRpc::get_ws() params {:?}", params);
        let ws = self.workspace.lock().await.clone();
        Ok(JsonValue::String(ws))
    }

    // RPCAPI:
    // Export tasks.
    // --> {"jsonrpc": "2.0", "method": "export_to", "params": [path], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "true", "id": 1}
    async fn export_to(&self, params: JsonValue) -> TaudResult<JsonValue> {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        debug!(target: "tau", "JsonRpc::export_to() params {:?}", params);

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
        debug!(target: "tau", "JsonRpc::import_from() params {:?}", params);

        if params.len() != 1 {
            return Err(TaudError::InvalidData("len of params should be 1".into()))
        }

        if !params[0].is_string() {
            return Err(TaudError::InvalidData("Invalid path".into()))
        }

        let path = params[0].get::<String>().unwrap();
        let path = expand_path(path)?.join("exported_tasks");
        let ws = self.workspace.lock().await.clone();

        let mut task_ids: Vec<u32> =
            MonthTasks::load_current_tasks(&self.dataset_path, ws.clone(), false)?
                .into_iter()
                .map(|t| t.id)
                .collect();

        let imported_tasks = MonthTasks::load_current_tasks(&path, ws.clone(), true)?;

        for mut task in imported_tasks {
            if MonthTasks::load_current_tasks(&self.dataset_path, ws.clone(), false)?
                .into_iter()
                .map(|t| t.ref_id)
                .any(|x| x == task.ref_id)
            {
                continue
            }

            task.id = find_free_id(&task_ids);
            task_ids.push(task.id);
            self.notify_queue_sender.send(task).await.map_err(Error::from)?;
        }
        Ok(JsonValue::Boolean(true))
    }

    fn load_task_by_id(&self, task_id: u32, ws: String) -> TaudResult<TaskInfo> {
        let tasks = MonthTasks::load_current_tasks(&self.dataset_path, ws, false)?;
        let task = tasks.into_iter().find(|t| (t.get_id()) == task_id);

        task.ok_or(TaudError::InvalidId)
    }

    fn check_params_for_update(
        &self,
        task_id: u32,
        fields: &HashMap<String, JsonValue>,
        ws: String,
    ) -> TaudResult<TaskInfo> {
        let mut task: TaskInfo = self.load_task_by_id(task_id, ws)?;

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
            // TODO: Why is this a double Option?
            let rank = {
                match fields["rank"] {
                    JsonValue::Null => None,
                    JsonValue::Number(rank) => Some(Some(rank as f32)),
                    _ => unreachable!(),
                }
            };

            if let Some(rank) = rank {
                task.set_rank(rank);
                match rank {
                    Some(r) => {
                        set_event(&mut task, "rank", &self.nickname, &r.to_string());
                    }
                    None => {
                        set_event(&mut task, "rank", &self.nickname, "None");
                    }
                }
            }
        }

        if fields.contains_key("due") {
            // TODO: Why is this a double Option?
            let due = {
                match &fields["due"] {
                    JsonValue::Null => None,
                    JsonValue::String(ts_str) => {
                        Some(Some(Timestamp(ts_str.parse::<u64>().unwrap())))
                    }
                    _ => unreachable!(),
                }
            };

            if let Some(d) = due {
                task.set_due(d);
                match d {
                    Some(v) => {
                        set_event(&mut task, "due", &self.nickname, &v.to_string());
                    }
                    None => {
                        set_event(&mut task, "due", &self.nickname, "None");
                    }
                }
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
