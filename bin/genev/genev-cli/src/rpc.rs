use log::debug;
use serde_json::json;

use darkfi::{
    event_graph::model::Event,
    rpc::{client::RpcClient, jsonrpc::JsonRequest},
    Result,
};

use crate::BaseEvent;

pub struct Gen {
    pub rpc_client: RpcClient,
}

impl Gen {
    pub async fn close_connection(&self) -> Result<()> {
        self.rpc_client.close().await
    }

    /// Add a new task.
    pub async fn add(&self, event: BaseEvent) -> Result<()> {
        let req = JsonRequest::new("add", json!([event]));
        let rep = self.rpc_client.request(req).await?;

        debug!("Got reply: {:?}", rep);
        Ok(())
    }

    /// Get current open tasks ids.
    pub async fn list(&self) -> Result<Vec<Event<BaseEvent>>> {
        let req = JsonRequest::new("list", json!([]));
        let rep = self.rpc_client.request(req).await?;

        debug!("reply: {:?}", rep);

        let bytes: Vec<u8> = serde_json::from_value(rep)?;
        let events: Vec<Event<BaseEvent>> = darkfi_serial::deserialize(&bytes)?;

        Ok(events)
    }
}
