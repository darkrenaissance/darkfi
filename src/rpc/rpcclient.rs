use log::{debug, error};
use serde_json::{json, Value};
use url::Url;

use crate::{Error, Result};

use super::jsonrpc::{self, ErrorCode, JsonRequest, JsonResult};

pub struct RpcClient {
    sender: async_channel::Sender<Value>,
    receiver: async_channel::Receiver<JsonResult>,
    stop_signal: async_channel::Sender<()>,
}

impl RpcClient {
    pub async fn new(url: Url) -> Result<Self> {
        let (sender, receiver, stop_signal) = jsonrpc::open_channels(&url).await?;
        Ok(Self { sender, receiver, stop_signal })
    }

    pub async fn close(&self) -> Result<()> {
        self.stop_signal.send(()).await?;
        Ok(())
    }

    pub async fn request(&self, value: JsonRequest) -> Result<Value> {
        let req_id = value.id.clone().as_u64().unwrap_or(0);
        let value = json!(value);

        // TODO proper error handling for closed channels
        // if the connection is closed the sender will get an error
        // for sending to closed channel
        let result = self.sender.send(value).await;
        if result.is_err() {
            error!("Unable to connect to the RPC server");
            return Err(Error::OperationFailed)
        }

        let reply = self.receiver.recv().await;

        // if the connection is closed the receiver will get an error
        // for waiting closed channel
        if reply.is_err() {
            error!("Unable to connect to the RPC server");
            return Err(Error::OperationFailed)
        }

        match reply? {
            JsonResult::Resp(r) => {
                // check if the ids match
                let resp_id = r.id.as_u64();

                if resp_id.is_none() {
                    let error = jsonrpc::error(ErrorCode::InvalidId, None, r.id);
                    self.stop_signal.send(()).await?;
                    return Err(Error::JsonRpcError(error.error.message.to_string()))
                }

                if resp_id.unwrap() != req_id {
                    let error = jsonrpc::error(
                        ErrorCode::InvalidId,
                        Some("Ids doesn't match".into()),
                        r.id,
                    );
                    self.stop_signal.send(()).await?;
                    return Err(Error::JsonRpcError(error.error.message.to_string()))
                }

                debug!(target: "RPC", "<-- {}", serde_json::to_string(&r)?);
                Ok(r.result)
            }

            JsonResult::Err(e) => {
                debug!(target: "RPC", "<-- {}", serde_json::to_string(&e)?);
                // close the server connection
                self.stop_signal.send(()).await?;
                Err(Error::JsonRpcError(e.error.message.to_string()))
            }

            JsonResult::Notif(n) => {
                debug!(target: "RPC", "<-- {}", serde_json::to_string(&n)?);
                Err(Error::JsonRpcError("Unexpected reply".to_string()))
            }
        }
    }
}
