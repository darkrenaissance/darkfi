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

use std::time::Instant;

use darkfi::{
    rpc::{
        client::RpcClient,
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResult},
        util::JsonValue,
    },
    system::{ExecutorPtr, Publisher, StoppableTask},
    Error, Result,
};
use log::{error, info};
use url::Url;

use crate::DamCli;

impl DamCli {
    /// Auxiliary function to ping configured damd daemon for liveness.
    pub async fn ping(&self) -> Result<()> {
        info!("Executing ping request to damd...");
        let latency = Instant::now();
        let rep = self.damd_daemon_request("ping", &JsonValue::Array(vec![])).await?;
        let latency = latency.elapsed();
        info!("Got reply: {rep:?}");
        info!("Latency: {latency:?}");
        Ok(())
    }

    /// Auxiliary function to execute a request towards the configured damd daemon JSON-RPC endpoint.
    pub async fn damd_daemon_request(&self, method: &str, params: &JsonValue) -> Result<JsonValue> {
        let req = JsonRequest::new(method, params.clone());
        let rep = self.rpc_client.request(req).await?;
        Ok(rep)
    }

    /// Subscribes to damd's JSON-RPC notification endpoints.
    pub async fn subscribe(&self, endpoint: &str, method: &str, ex: &ExecutorPtr) -> Result<()> {
        info!("Subscribing to receive notifications for: {method}");
        let endpoint = Url::parse(endpoint)?;
        let _method = String::from(method);
        let publisher = Publisher::new();
        let subscription = publisher.clone().subscribe().await;
        let _publisher = publisher.clone();
        let _ex = ex.clone();
        StoppableTask::new().start(
            // Weird hack to prevent lifetimes hell
            async move {
                let rpc_client = RpcClient::new(endpoint, _ex).await?;
                let req = JsonRequest::new(&_method, JsonValue::Array(vec![]));
                rpc_client.subscribe(req, _publisher).await
            },
            |res| async move {
                match res {
                    Ok(()) => { /* Do nothing */ }
                    Err(e) => {
                        error!("[subscribe] JSON-RPC server error: {e:?}");
                        publisher
                            .notify(JsonResult::Error(JsonError::new(
                                ErrorCode::InternalError,
                                None,
                                0,
                            )))
                            .await;
                    }
                }
            },
            Error::RpcServerStopped,
            ex.clone(),
        );
        info!("Detached subscription to background");
        info!("All is good. Waiting for new notifications...");

        let e = loop {
            match subscription.receive().await {
                JsonResult::Notification(n) => {
                    info!("Got notification from subscription");
                    if n.method != method {
                        break Error::UnexpectedJsonRpc(format!(
                            "Got foreign notification from damd: {}",
                            n.method
                        ))
                    }

                    // Verify parameters
                    if !n.params.is_array() {
                        break Error::UnexpectedJsonRpc(
                            "Received notification params are not an array".to_string(),
                        )
                    }
                    let params = n.params.get::<Vec<JsonValue>>().unwrap();
                    if params.is_empty() {
                        break Error::UnexpectedJsonRpc(
                            "Notification parameters are empty".to_string(),
                        )
                    }

                    for param in params {
                        let param = param.get::<String>().unwrap();
                        println!("Notification: {param}");
                    }
                }

                JsonResult::Error(e) => {
                    // Some error happened in the transmission
                    break Error::UnexpectedJsonRpc(format!("Got error from JSON-RPC: {e:?}"))
                }

                x => {
                    // And this is weird
                    break Error::UnexpectedJsonRpc(format!(
                        "Got unexpected data from JSON-RPC: {x:?}"
                    ))
                }
            }
        };

        Err(e)
    }
}
