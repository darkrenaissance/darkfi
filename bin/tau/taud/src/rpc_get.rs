use log::debug;
use serde_json::{json, Value};

use crate::{
    error::{TaudError, TaudResult},
    month_tasks::MonthTasks,
    task_info::TaskInfo,
    JsonRpcInterface,
};

impl JsonRpcInterface {
    // RPCAPI:
    // List tasks
    // --> {"jsonrpc": "2.0", "method": "get_ids", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": [task_id, ...], "id": 1}
    pub async fn get_ids(&self, params: Value) -> TaudResult<Value> {
        debug!(target: "tau", "JsonRpc::get_ids() params {}", params);
        let tasks = MonthTasks::load_current_open_tasks(&self.dataset_path)?;
        let task_ids: Vec<u32> = tasks.iter().map(|task| task.get_id()).collect();
        Ok(json!(task_ids))
    }

    // RPCAPI:
    // Get a task by id.
    // --> {"jsonrpc": "2.0", "method": "get_task_by_id", "params": [task_id], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "task", "id": 1}
    pub async fn get_task_by_id(&self, params: Value) -> TaudResult<Value> {
        debug!(target: "tau", "JsonRpc::get_task_by_id() params {}", params);
        let args = params.as_array().unwrap();

        if args.len() != 1 {
            return Err(TaudError::InvalidData("len of params should be 1".into()))
        }

        let task: TaskInfo = self.load_task_by_id(&args[0])?;

        Ok(json!(task))
    }
}
