use log::debug;
use serde_json::{json, Value};

use darkfi::{util::Timestamp, Error};

use crate::{
    error::{TaudError, TaudResult},
    task_info::{Comment, TaskInfo},
    JsonRpcInterface,
};

impl JsonRpcInterface {
    // RPCAPI:
    // Update task and returns `true` upon success.
    // --> {"jsonrpc": "2.0", "method": "update", "params": [task_id, {"title": "new title"} ], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    pub async fn update(&self, params: Value) -> TaudResult<Value> {
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
    pub async fn set_state(&self, params: Value) -> TaudResult<Value> {
        // TODO: BUG: Validate that the state string is correct and not something arbitrary

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
    pub async fn set_comment(&self, params: Value) -> TaudResult<Value> {
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
