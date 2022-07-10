use std::path::PathBuf;

use async_trait::async_trait;
use log::debug;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use darkfi::{
    rpc::{
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResult},
        server::RequestHandler,
    },
    util::Timestamp,
};

use crate::{
    error::{to_json_result, TaudError, TaudResult},
    month_tasks::MonthTasks,
    task_info::{Comment, TaskInfo},
};

pub struct JsonRpcInterface {
    dataset_path: PathBuf,
    nickname: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct BaseTaskInfo {
    title: String,
    desc: String,
    assign: Vec<String>,
    project: Vec<String>,
    due: Option<Timestamp>,
    rank: Option<f32>,
}

#[async_trait]
impl RequestHandler for JsonRpcInterface {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        if !req.params.is_array() {
            return JsonError::new(ErrorCode::InvalidParams, None, req.id).into()
        }

        let params = req.params.as_array().unwrap();

        let rep = match req.method.as_str() {
            Some("add") => self.add(params).await,
            Some("get_ids") => self.get_ids(params).await,
            Some("update") => self.update(params).await,
            Some("set_state") => self.set_state(params).await,
            Some("set_comment") => self.set_comment(params).await,
            Some("get_task_by_id") => self.get_task_by_id(params).await,
            Some(_) | None => return JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
        };

        to_json_result(rep, req.id)
    }
}

impl JsonRpcInterface {
    pub fn new(dataset_path: PathBuf, nickname: String) -> Self {
        Self { dataset_path, nickname }
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
    async fn add(&self, params: &[Value]) -> TaudResult<Value> {
        debug!(target: "tau", "JsonRpc::add() params {:?}", params);

        let task: BaseTaskInfo = serde_json::from_value(params[0].clone())?;
        let mut new_task: TaskInfo = TaskInfo::new(
            &task.title,
            &task.desc,
            &self.nickname,
            task.due,
            task.rank.unwrap_or(0.0),
            &self.dataset_path,
        )?;
        new_task.set_project(&task.project);
        new_task.set_assign(&task.assign);

        new_task.save(&self.dataset_path)?;
        Ok(json!(true))
    }

    // RPCAPI:
    // List tasks
    // --> {"jsonrpc": "2.0", "method": "get_ids", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": [task_id, ...], "id": 1}
    async fn get_ids(&self, params: &[Value]) -> TaudResult<Value> {
        debug!(target: "tau", "JsonRpc::get_ids() params {:?}", params);
        let tasks = MonthTasks::load_current_open_tasks(&self.dataset_path)?;
        let task_ids: Vec<u32> = tasks.iter().map(|task| task.get_id()).collect();
        Ok(json!(task_ids))
    }

    // RPCAPI:
    // Update task and returns `true` upon success.
    // --> {"jsonrpc": "2.0", "method": "update", "params": [task_id, {"title": "new title"} ], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    async fn update(&self, params: &[Value]) -> TaudResult<Value> {
        debug!(target: "tau", "JsonRpc::update() params {:?}", params);

        if params.len() != 2 {
            return Err(TaudError::InvalidData("len of params should be 2".into()))
        }

        let task = self.check_params_for_update(&params[0], &params[1])?;
        task.save(&self.dataset_path)?;
        Ok(json!(true))
    }

    // RPCAPI:
    // Set state for a task and returns `true` upon success.
    // --> {"jsonrpc": "2.0", "method": "set_state", "params": [task_id, state], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    async fn set_state(&self, params: &[Value]) -> TaudResult<Value> {
        // Allowed states for a task
        let states = ["stop", "start", "open", "pause"];

        debug!(target: "tau", "JsonRpc::set_state() params {:?}", params);

        if params.len() != 2 {
            return Err(TaudError::InvalidData("len of params should be 2".into()))
        }

        let state: String = serde_json::from_value(params[1].clone())?;

        let mut task: TaskInfo = self.load_task_by_id(&params[0])?;

        if states.contains(&state.as_str()) {
            task.set_state(&state);
        }

        task.save(&self.dataset_path)?;

        Ok(json!(true))
    }

    // RPCAPI:
    // Set comment for a task and returns `true` upon success.
    // --> {"jsonrpc": "2.0", "method": "set_comment", "params": [task_id, comment_content], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    async fn set_comment(&self, params: &[Value]) -> TaudResult<Value> {
        debug!(target: "tau", "JsonRpc::set_comment() params {:?}", params);

        if params.len() != 2 {
            return Err(TaudError::InvalidData("len of params should be 3".into()))
        }

        let comment_content: String = serde_json::from_value(params[1].clone())?;

        let mut task: TaskInfo = self.load_task_by_id(&params[0])?;
        task.set_comment(Comment::new(&comment_content, &self.nickname));

        task.save(&self.dataset_path)?;

        Ok(json!(true))
    }

    // RPCAPI:
    // Get a task by id.
    // --> {"jsonrpc": "2.0", "method": "get_task_by_id", "params": [task_id], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "task", "id": 1}
    async fn get_task_by_id(&self, params: &[Value]) -> TaudResult<Value> {
        debug!(target: "tau", "JsonRpc::get_task_by_id() params {:?}", params);

        if params.len() != 1 {
            return Err(TaudError::InvalidData("len of params should be 1".into()))
        }

        let task: TaskInfo = self.load_task_by_id(&params[0])?;

        Ok(json!(task))
    }

    fn load_task_by_id(&self, task_id: &Value) -> TaudResult<TaskInfo> {
        let task_id: u64 = serde_json::from_value(task_id.clone())?;

        let tasks = MonthTasks::load_current_open_tasks(&self.dataset_path)?;
        let task = tasks.into_iter().find(|t| (t.get_id() as u64) == task_id);

        task.ok_or(TaudError::InvalidId)
    }

    fn check_params_for_update(&self, task_id: &Value, fields: &Value) -> TaudResult<TaskInfo> {
        let mut task: TaskInfo = self.load_task_by_id(task_id)?;

        if !fields.is_object() {
            return Err(TaudError::InvalidData("Invalid task's data".into()))
        }

        let fields = fields.as_object().unwrap();

        if fields.contains_key("title") {
            let title = fields.get("title").unwrap().clone();
            let title: String = serde_json::from_value(title)?;
            if !title.is_empty() {
                task.set_title(&title);
            }
        }

        if fields.contains_key("desc") {
            let description = fields.get("description");
            if let Some(description) = description {
                let description: String = serde_json::from_value(description.clone())?;
                task.set_desc(&description);
            }
        }

        if fields.contains_key("rank") {
            let rank_opt = fields.get("rank");
            if let Some(rank) = rank_opt {
                let rank: Option<f32> = serde_json::from_value(rank.clone())?;
                if let Some(r) = rank {
                    task.set_rank(r);
                }
            }
        }

        if fields.contains_key("due") {
            let due = fields.get("due").unwrap().clone();
            let due: Option<Option<Timestamp>> = serde_json::from_value(due)?;
            if let Some(d) = due {
                task.set_due(d);
            }
        }

        if fields.contains_key("assign") {
            let assign = fields.get("assign").unwrap().clone();
            let assign: Vec<String> = serde_json::from_value(assign)?;
            if !assign.is_empty() {
                task.set_assign(&assign);
            }
        }

        if fields.contains_key("project") {
            let project = fields.get("project").unwrap().clone();
            let project: Vec<String> = serde_json::from_value(project)?;
            if !project.is_empty() {
                task.set_project(&project);
            }
        }

        Ok(task)
    }
}
