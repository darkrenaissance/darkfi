use std::{path::PathBuf, sync::Arc};

use async_executor::Executor;
use async_trait::async_trait;
use log::debug;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use darkfi::{
    rpc::{
        jsonrpc::{error as jsonerr, ErrorCode, JsonRequest, JsonResult},
        rpcserver::RequestHandler,
    },
    Error,
};

use crate::{
    error::{to_json_result, TaudError, TaudResult},
    month_tasks::MonthTasks,
    task_info::{Comment, TaskInfo},
    util::Timestamp,
};

pub struct JsonRpcInterface {
    dataset_path: PathBuf,
    notify_queue_sender: async_channel::Sender<Option<TaskInfo>>,
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
    async fn handle_request(&self, req: JsonRequest, _executor: Arc<Executor<'_>>) -> JsonResult {
        if req.params.as_array().is_none() {
            return JsonResult::Err(jsonerr(ErrorCode::InvalidParams, None, req.id))
        }

        if self.notify_queue_sender.send(None).await.is_err() {
            return JsonResult::Err(jsonerr(ErrorCode::InternalError, None, req.id))
        }

        debug!(target: "RPC", "--> {}", serde_json::to_string(&req).unwrap());

        let rep = match req.method.as_str() {
            Some("add") => self.add(req.params).await,
            Some("list") => self.list(req.params).await,
            Some("update") => self.update(req.params).await,
            Some("set_state") => self.set_state(req.params).await,
            Some("set_comment") => self.set_comment(req.params).await,
            Some("get_task_by_id") => self.get_task_by_id(req.params).await,
            Some(_) | None => {
                return JsonResult::Err(jsonerr(ErrorCode::MethodNotFound, None, req.id))
            }
        };

        to_json_result(rep, req.id)
    }
}

impl JsonRpcInterface {
    pub fn new(
        notify_queue_sender: async_channel::Sender<Option<TaskInfo>>,
        dataset_path: PathBuf,
        nickname: String,
    ) -> Self {
        Self { notify_queue_sender, dataset_path, nickname }
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
    async fn add(&self, params: Value) -> TaudResult<Value> {
        debug!(target: "tau", "JsonRpc::add() params {}", params);
        let args = params.as_array().unwrap();

        let task: BaseTaskInfo = serde_json::from_value(args[0].clone())?;
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

        self.notify_queue_sender.send(Some(new_task)).await.map_err(Error::from)?;

        Ok(json!(true))
    }

    // RPCAPI:
    // List tasks
    // --> {"jsonrpc": "2.0", "method": "list", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": [task, ...], "id": 1}
    async fn list(&self, params: Value) -> TaudResult<Value> {
        debug!(target: "tau", "JsonRpc::list() params {}", params);
        let tks = MonthTasks::load_current_open_tasks(&self.dataset_path)?;
        Ok(json!(tks))
    }

    // RPCAPI:
    // Update task and returns `true` upon success.
    // --> {"jsonrpc": "2.0", "method": "update", "params": [task_id, {"title": "new title"} ], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    async fn update(&self, params: Value) -> TaudResult<Value> {
        debug!(target: "tau", "JsonRpc::update() params {}", params);
        let args = params.as_array().unwrap();

        if args.len() != 2 {
            return Err(TaudError::InvalidData("len of params should be 2".into()))
        }

        let task = self.check_params_for_update(&args[0], &args[1])?;

        self.notify_queue_sender.send(Some(task)).await.map_err(Error::from)?;

        Ok(json!(true))
    }

    // RPCAPI:
    // Set state for a task and returns `true` upon success.
    // --> {"jsonrpc": "2.0", "method": "set_state", "params": [task_id, state], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    async fn set_state(&self, params: Value) -> TaudResult<Value> {
        debug!(target: "tau", "JsonRpc::set_state() params {}", params);
        let args = params.as_array().unwrap();

        if args.len() != 2 {
            return Err(TaudError::InvalidData("len of params should be 2".into()))
        }

        let state: String = serde_json::from_value(args[1].clone())?;

        let mut task: TaskInfo = self.load_task_by_id(&args[0])?;
        task.set_state(&state);

        self.notify_queue_sender.send(Some(task)).await.map_err(Error::from)?;

        Ok(json!(true))
    }

    // RPCAPI:
    // Set comment for a task and returns `true` upon success.
    // --> {"jsonrpc": "2.0", "method": "set_comment", "params": [task_id, comment_content], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    async fn set_comment(&self, params: Value) -> TaudResult<Value> {
        debug!(target: "tau", "JsonRpc::set_comment() params {}", params);
        let args = params.as_array().unwrap();

        if args.len() != 2 {
            return Err(TaudError::InvalidData("len of params should be 3".into()))
        }

        let comment_content: String = serde_json::from_value(args[1].clone())?;

        let mut task: TaskInfo = self.load_task_by_id(&args[0])?;
        task.set_comment(Comment::new(&comment_content, &self.nickname));

        self.notify_queue_sender.send(Some(task)).await.map_err(Error::from)?;
        Ok(json!(true))
    }

    // RPCAPI:
    // Get a task by id.
    // --> {"jsonrpc": "2.0", "method": "get_task_by_id", "params": [task_id], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "task", "id": 1}
    async fn get_task_by_id(&self, params: Value) -> TaudResult<Value> {
        debug!(target: "tau", "JsonRpc::get_task_by_id() params {}", params);
        let args = params.as_array().unwrap();

        if args.len() != 1 {
            return Err(TaudError::InvalidData("len of params should be 1".into()))
        }

        let task: TaskInfo = self.load_task_by_id(&args[0])?;

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

        if !fields.is_array() {
            return Err(TaudError::InvalidData("Invalid task's data".into()))
        }

        let fields = fields.as_array().unwrap();

        for field in fields {
            if !field.is_object() {
                return Err(TaudError::InvalidData("Invalid task's fields".into()))
            }

            let field = field.as_object().unwrap();

            if field.contains_key("title") {
                let title = field.get("title").unwrap().clone();
                let title: String = serde_json::from_value(title)?;
                if !title.is_empty() {
                    task.set_title(&title);
                }
            }

            if field.contains_key("desc") {
                let description = field.get("description");
                if let Some(description) = description {
                    let description: String = serde_json::from_value(description.clone())?;
                    task.set_desc(&description);
                }
            }

            if field.contains_key("rank") {
                let rank_opt = field.get("rank");
                if let Some(rank) = rank_opt {
                    let rank: Option<f32> = serde_json::from_value(rank.clone())?;
                    if rank.is_some() {
                        task.set_rank(rank.unwrap());
                    }
                }
            }

            if field.contains_key("due") {
                let due = field.get("due").unwrap().clone();
                let due: Option<Option<Timestamp>> = serde_json::from_value(due)?;
                if let Some(d) = due {
                    task.set_due(d);
                }
            }

            if field.contains_key("assign") {
                let assign = field.get("assign").unwrap().clone();
                let assign: Vec<String> = serde_json::from_value(assign)?;
                if !assign.is_empty() {
                    task.set_assign(&assign);
                }
            }

            if field.contains_key("project") {
                let project = field.get("project").unwrap().clone();
                let project: Vec<String> = serde_json::from_value(project)?;
                if !project.is_empty() {
                    task.set_project(&project);
                }
            }
        }
        Ok(task)
    }
}
