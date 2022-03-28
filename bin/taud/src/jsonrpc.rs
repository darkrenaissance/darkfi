use std::sync::Arc;

use async_executor::Executor;
use async_trait::async_trait;
use log::debug;
use serde_json::{json, Value};

use darkfi::{
    rpc::{
        jsonrpc::{error as jsonerr, response as jsonresp, ErrorCode, JsonRequest, JsonResult},
        rpcserver::RequestHandler,
    },
    Error,
};

use crate::{
    error::TaudResult,
    month_tasks::MonthTasks,
    task_info::{Comment, TaskInfo},
    util::{get_current_time, Settings, Timestamp},
};

pub struct JsonRpcInterface {
    settings: Settings,
    notify_queue_sender: async_channel::Sender<TaskInfo>,
}

#[async_trait]
impl RequestHandler for JsonRpcInterface {
    async fn handle_request(&self, req: JsonRequest, _executor: Arc<Executor<'_>>) -> JsonResult {
        if req.params.as_array().is_none() {
            return JsonResult::Err(jsonerr(ErrorCode::InvalidParams, None, req.id))
        }

        debug!(target: "RPC", "--> {}", serde_json::to_string(&req).unwrap());

        match req.method.as_str() {
            Some("add") => return self.add(req.id, req.params).await,
            Some("list") => return self.list(req.id, req.params).await,
            Some("update") => return self.update(req.id, req.params).await,
            Some("get_state") => return self.get_state(req.id, req.params).await,
            Some("set_state") => return self.set_state(req.id, req.params).await,
            Some("set_comment") => return self.set_comment(req.id, req.params).await,
            Some(_) | None => {
                return JsonResult::Err(jsonerr(ErrorCode::MethodNotFound, None, req.id))
            }
        }
    }
}

impl JsonRpcInterface {
    pub fn new(notify_queue_sender: async_channel::Sender<TaskInfo>, settings: Settings) -> Self {
        Self { notify_queue_sender, settings }
    }

    // RPCAPI:
    // Add new task and returns `true` upon success.
    // --> {"jsonrpc": "2.0", "method": "add", "params": ["title", "desc", ["assign"], ["project"], "due", "rank"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    async fn add(&self, id: Value, params: Value) -> JsonResult {
        let args = params.as_array().unwrap();

        if args.len() != 6 {
            return JsonResult::Err(jsonerr(ErrorCode::InvalidParams, None, id))
        }

        let mut task: TaskInfo;

        match (args[0].as_str(), args[1].as_str(), args[5].as_u64()) {
            (Some(title), Some(desc), Some(rank)) => {
                let due: Option<Timestamp> = if args[4].is_i64() {
                    let timestamp = args[4].as_i64().unwrap();
                    let timestamp = Timestamp(timestamp);

                    if timestamp < get_current_time() {
                        return JsonResult::Err(jsonerr(
                            ErrorCode::InvalidParams,
                            Some("invalid due date".into()),
                            id,
                        ))
                    }

                    Some(timestamp)
                } else {
                    None
                };

                match TaskInfo::new(title, desc, due, rank as u32, &self.settings) {
                    Ok(t) => task = t,
                    Err(e) => {
                        return JsonResult::Err(jsonerr(
                            ErrorCode::InternalError,
                            Some(e.to_string()),
                            id,
                        ))
                    }
                }
            }
            (None, _, _) => {
                return JsonResult::Err(jsonerr(
                    ErrorCode::InvalidParams,
                    Some("invalid title".into()),
                    id,
                ))
            }
            (_, None, _) => {
                return JsonResult::Err(jsonerr(
                    ErrorCode::InvalidParams,
                    Some("invalid desc".into()),
                    id,
                ))
            }
            (_, _, None) => {
                return JsonResult::Err(jsonerr(
                    ErrorCode::InvalidParams,
                    Some("invalid rank".into()),
                    id,
                ))
            }
        }

        let assign = args[2].as_array();
        if assign.is_some() && !assign.unwrap().is_empty() {
            task.set_assign(
                &assign
                    .unwrap()
                    .iter()
                    .filter(|a| a.as_str().is_some())
                    .map(|a| a.as_str().unwrap().to_string())
                    .collect(),
            );
        }

        let project = args[3].as_array();
        if project.is_some() && !project.unwrap().is_empty() {
            task.set_project(
                &project
                    .unwrap()
                    .iter()
                    .filter(|p| p.as_str().is_some())
                    .map(|p| p.as_str().unwrap().to_string())
                    .collect(),
            );
        }

        let result: TaudResult<()> = async move {
            task.save()?;
            task.activate()?;
            self.notify_queue_sender.send(task).await.map_err(Error::from)?;
            Ok(())
        }
        .await;

        match result {
            Ok(()) => JsonResult::Resp(jsonresp(json!(true), id)),
            Err(e) => JsonResult::Err(jsonerr(ErrorCode::InternalError, Some(e.to_string()), id)),
        }
    }

    // RPCAPI:
    // List tasks
    // --> {"jsonrpc": "2.0", "method": "list", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": [task, ...], "id": 1}
    async fn list(&self, id: Value, _params: Value) -> JsonResult {
        match MonthTasks::load_current_open_tasks(&self.settings) {
            Ok(tks) => JsonResult::Resp(jsonresp(json!(tks), id)),
            Err(e) => JsonResult::Err(jsonerr(ErrorCode::InternalError, Some(e.to_string()), id)),
        }
    }

    // RPCAPI:
    // Update task and returns `true` upon success.
    // --> {"jsonrpc": "2.0", "method": "update", "params": [task_id, {"title": "new title"} ], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    async fn update(&self, id: Value, params: Value) -> JsonResult {
        let args = params.as_array().unwrap();

        if args.len() != 2 {
            return JsonResult::Err(jsonerr(ErrorCode::InvalidParams, None, id))
        }

        if !args[0].is_u64() {
            return JsonResult::Err(jsonerr(ErrorCode::InvalidParams, Some("invalid id".into()), id))
        }

        let task_id = args[0].as_u64().unwrap();

        let task = match self.check_data_for_update(task_id, args[1].clone()) {
            Ok(t) => t,
            Err(e) => {
                return JsonResult::Err(jsonerr(ErrorCode::InvalidParams, Some(e.to_string()), id))
            }
        };

        let result: TaudResult<()> = async move {
            task.save()?;
            self.notify_queue_sender.send(task).await.map_err(Error::from)?;
            Ok(())
        }
        .await;

        match result {
            Ok(()) => JsonResult::Resp(jsonresp(json!(true), id)),
            Err(e) => JsonResult::Err(jsonerr(ErrorCode::InternalError, Some(e.to_string()), id)),
        }
    }

    // RPCAPI:
    // Get task's state.
    // --> {"jsonrpc": "2.0", "method": "get_state", "params": [task_id], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "state", "id": 1}
    async fn get_state(&self, id: Value, params: Value) -> JsonResult {
        let args = params.as_array().unwrap();

        if !args[0].is_u64() {
            return JsonResult::Err(jsonerr(ErrorCode::InvalidParams, Some("invalid id".into()), id))
        }

        let task_id = args[0].as_u64().unwrap();

        let task: TaskInfo = match self.load_task_by_id(task_id) {
            Ok(t) => t,
            Err(e) => return JsonResult::Err(jsonerr(ErrorCode::InvalidParams, Some(e), id)),
        };

        JsonResult::Resp(jsonresp(json!(task.get_state()), id))
    }

    // RPCAPI:
    // Set state for a task and returns `true` upon success.
    // --> {"jsonrpc": "2.0", "method": "set_state", "params": [task_id, state], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    async fn set_state(&self, id: Value, params: Value) -> JsonResult {
        let args = params.as_array().unwrap();

        if !args[0].is_u64() {
            return JsonResult::Err(jsonerr(ErrorCode::InvalidParams, Some("invalid id".into()), id))
        }

        if !args[1].is_string() {
            return JsonResult::Err(jsonerr(
                ErrorCode::InvalidParams,
                Some("invalid state".into()),
                id,
            ))
        }

        let task_id = args[0].as_u64().unwrap();
        let state = args[1].as_str().unwrap();

        let mut task: TaskInfo = match self.load_task_by_id(task_id) {
            Ok(t) => t,
            Err(e) => return JsonResult::Err(jsonerr(ErrorCode::InvalidParams, Some(e), id)),
        };

        task.set_state(state);

        let result: TaudResult<()> = async move {
            task.save()?;
            self.notify_queue_sender.send(task).await.map_err(Error::from)?;
            Ok(())
        }
        .await;

        match result {
            Ok(()) => JsonResult::Resp(jsonresp(json!(true), id)),
            Err(e) => JsonResult::Err(jsonerr(ErrorCode::InternalError, Some(e.to_string()), id)),
        }
    }

    // RPCAPI:
    // Set comment for a task and returns `true` upon success.
    // --> {"jsonrpc": "2.0", "method": "set_comment", "params": [task_id, comment_author, comment_content], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    async fn set_comment(&self, id: Value, params: Value) -> JsonResult {
        let args = params.as_array().unwrap();

        if !args[0].is_u64() {
            return JsonResult::Err(jsonerr(ErrorCode::InvalidParams, Some("invalid id".into()), id))
        }

        if !args[1].is_string() {
            return JsonResult::Err(jsonerr(
                ErrorCode::InvalidParams,
                Some("invalid comment author".into()),
                id,
            ))
        }

        if !args[2].is_string() {
            return JsonResult::Err(jsonerr(
                ErrorCode::InvalidParams,
                Some("invalid comment content".into()),
                id,
            ))
        }

        let task_id = args[0].as_u64().unwrap();
        let comment_author = args[1].as_str().unwrap();
        let comment_content = args[2].as_str().unwrap();

        let mut task: TaskInfo = match self.load_task_by_id(task_id) {
            Ok(t) => t,
            Err(e) => return JsonResult::Err(jsonerr(ErrorCode::InvalidParams, Some(e), id)),
        };

        task.set_comment(Comment::new(comment_content, comment_author));

        let result: TaudResult<()> = async move {
            task.save()?;
            self.notify_queue_sender.send(task).await.map_err(Error::from)?;
            Ok(())
        }
        .await;

        match result {
            Ok(()) => JsonResult::Resp(jsonresp(json!(true), id)),
            Err(e) => JsonResult::Err(jsonerr(ErrorCode::InternalError, Some(e.to_string()), id)),
        }
    }

    fn load_task_by_id(&self, task_id: u64) -> std::result::Result<TaskInfo, String> {
        let tasks: Vec<TaskInfo> = match MonthTasks::load_current_open_tasks(&self.settings) {
            Ok(v) => v,
            Err(e) => return Err(e.to_string()),
        };

        let task = tasks.into_iter().find(|t| (t.get_id() as u64) == task_id);

        if task.is_none() {
            return Err("Didn't find a task with the provided id".into())
        }

        Ok(task.unwrap())
    }

    fn check_data_for_update(
        &self,
        task_id: u64,
        data: Value,
    ) -> std::result::Result<TaskInfo, String> {
        let mut task: TaskInfo = self.load_task_by_id(task_id)?;

        if !data.is_object() {
            return Err("invalid data for update".into())
        }

        let data = data.as_object().unwrap();

        if data.contains_key("title") {
            let title = data
                .get("title")
                .ok_or("error parsing title")?
                .as_str()
                .ok_or("invalid value for title")?;
            task.set_title(title);
        }

        if data.contains_key("description") {
            let description = data
                .get("description")
                .ok_or("error parsing description")?
                .as_str()
                .ok_or("invalid value for description")?;
            task.set_desc(description);
        }

        if data.contains_key("rank") {
            let rank = data
                .get("rank")
                .ok_or("error parsing rank")?
                .as_u64()
                .ok_or("invalid value for rank")?;

            task.set_rank(rank as u32);
        }

        if data.contains_key("due") {
            if let Some(due) = data.get("due").ok_or("error parsing due")?.as_i64() {
                task.set_due(Some(Timestamp(due)));
            } else {
                task.set_due(None);
            }
        }

        if data.contains_key("assign") {
            task.set_assign(
                &data
                    .get("assign")
                    .ok_or("error parsing assign")?
                    .as_array()
                    .ok_or("invalid value for assign")?
                    .iter()
                    .filter(|a| a.as_str().is_some())
                    .map(|a| a.as_str().unwrap().to_string())
                    .collect(),
            );
        }

        if data.contains_key("project") {
            task.set_project(
                &data
                    .get("project")
                    .ok_or("error parsing project")?
                    .as_array()
                    .ok_or("invalid value for project")?
                    .iter()
                    .filter(|p| p.as_str().is_some())
                    .map(|p| p.as_str().unwrap().to_string())
                    .collect(),
            );
        }

        Ok(task)
    }
}
