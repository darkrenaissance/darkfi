use log::debug;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use darkfi::{util::Timestamp, Error};

use crate::{
    error::{TaudError, TaudResult},
    task_info::TaskInfo,
    JsonRpcInterface,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct BaseTaskInfo {
    title: String,
    desc: String,
    assign: Vec<String>,
    project: Vec<String>,
    due: Option<Timestamp>,
    rank: Option<f32>,
}

impl JsonRpcInterface {
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
    pub async fn add(&self, params: Value) -> TaudResult<Value> {
        debug!(target: "tau", "JsonRpc::add() params {}", params);

        if !params.is_array() {
            return Err(TaudError::InvalidData("params is not an array".into()))
        }

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
}
