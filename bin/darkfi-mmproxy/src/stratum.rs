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

use std::{collections::HashMap, sync::Arc, time::Duration};

use darkfi::{
    rpc::{
        jsonrpc::{ErrorCode, JsonError, JsonResponse, JsonResult, JsonSubscriber},
        util::JsonValue,
    },
    system::{timeout::timeout, StoppableTask},
    Error, Result,
};
use log::{debug, error, info, warn};
use smol::{channel, lock::RwLock};
use uuid::Uuid;

use super::{error::RpcError, MiningProxy, Worker};

/// Algo string representing Monero's RandomX
pub const RANDOMX_ALGO: &str = "rx/0";

impl MiningProxy {
    /// Background task listening for keepalives from a worker, if timeout is reached
    /// the worker will be dropped.
    async fn keepalive_task(
        workers: Arc<RwLock<HashMap<Uuid, Worker>>>,
        uuid: Uuid,
        ka_recv: channel::Receiver<()>,
    ) -> Result<()> {
        debug!("Spawned keepalive_task for worker {}", uuid);
        const TIMEOUT: Duration = Duration::from_secs(60);

        loop {
            let Ok(r) = timeout(TIMEOUT, ka_recv.recv()).await else {
                // Timeout, remove worker
                warn!("keepalive_task {} worker timed out", uuid);
                workers.write().await.remove(&uuid);
                break
            };

            match r {
                Ok(()) => {
                    debug!("keepalive_task {} got ping", uuid);
                    continue
                }
                Err(e) => {
                    error!("keepalive_task {} channel recv error: {}", uuid, e);
                    workers.write().await.remove(&uuid);
                    break
                }
            }
        }

        Ok(())
    }

    /// Stratum login method. `darkfi-mmproxy` will check that it is a valid worker
    /// login, and will also search for `RANDOMX_ALGO`.
    /// TODO: More proper error codes
    pub async fn stratum_login(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 1 || !params[0].is_object() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        let params = params[0].get::<HashMap<String, JsonValue>>().unwrap();

        if !params.contains_key("login") ||
            !params.contains_key("pass") ||
            !params.contains_key("agent") ||
            !params.contains_key("algo")
        {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        let Some(login) = params["login"].get::<String>() else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };

        let Some(pass) = params["pass"].get::<String>() else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };

        let Some(agent) = params["agent"].get::<String>() else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };

        let Some(algos) = params["algo"].get::<Vec<JsonValue>>() else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };

        // We'll only support rx/0 algo.
        let mut found_xmr_algo = false;
        for algo in algos {
            if !algo.is_string() {
                return JsonError::new(ErrorCode::InvalidParams, None, id).into()
            }

            if algo.get::<String>().unwrap() == RANDOMX_ALGO {
                found_xmr_algo = true;
                break
            }
        }

        if !found_xmr_algo {
            return JsonError::new(
                RpcError::UnsupportedMiningAlgo.into(),
                Some("Unsupported mining algo".to_string()),
                id,
            )
            .into()
        }

        // Check valid login
        let Some(known_pass) = self.logins.get(login) else {
            return JsonError::new(
                RpcError::InvalidWorkerLogin.into(),
                Some("Unknown worker username".to_string()),
                id,
            )
            .into()
        };

        if known_pass != pass {
            return JsonError::new(
                RpcError::InvalidWorkerLogin.into(),
                Some("Invalid worker password".to_string()),
                id,
            )
            .into()
        }

        // Login success, generate UUID
        let uuid = Uuid::new_v4();

        // Create job subscriber
        let job_sub = JsonSubscriber::new("job");

        // Create keepalive channel
        let (ka_send, ka_recv) = channel::unbounded();

        // Create background keepalive task
        let ka_task = StoppableTask::new();

        // Create worker
        let worker = Worker::new(job_sub, ka_send, ka_task.clone());

        // Insert into connections map
        self.workers.write().await.insert(uuid, worker);

        // Spawn background task
        ka_task.start(
            Self::keepalive_task(self.workers.clone(), uuid.clone(), ka_recv),
            move |_| async move { debug!("keepalive_task for {} exited", uuid) },
            Error::DetachedTaskStopped,
            self.executor.clone(),
        );

        info!("Added worker {} ({})", login, uuid);

        // TODO: Send current job
        return JsonResponse::new(
            JsonValue::Object(HashMap::from([(
                "status".to_string(),
                JsonValue::String("KEEPALIVED".to_string()),
            )])),
            id,
        )
        .into()
    }

    pub async fn stratum_submit(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 1 || !params[0].is_object() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        let params = params[0].get::<HashMap<String, JsonValue>>().unwrap();

        if !params.contains_key("id") ||
            !params.contains_key("job_id") ||
            !params.contains_key("nonce") ||
            !params.contains_key("result")
        {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        let Some(uuid) = params["id"].get::<String>() else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };

        let Some(job_id) = params["job_id"].get::<String>() else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };

        let Some(nonce) = params["nonce"].get::<String>() else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };

        let Some(result) = params["result"].get::<String>() else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };

        todo!()
    }

    /// Non standard but widely supported protocol extension. Miner sends `keepalived`
    /// to prevent connection timeout.
    pub async fn stratum_keepalived(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 1 || !params[0].is_object() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        let params = params[0].get::<HashMap<String, JsonValue>>().unwrap();

        if !params.contains_key("id") {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        let Some(uuid) = params["id"].get::<String>() else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };

        let Ok(uuid) = Uuid::try_from(uuid.as_str()) else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };

        // Ping the keepalive task
        let workers = self.workers.read().await;
        let Some(worker) = workers.get(&uuid) else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };

        if let Err(e) = worker.ka_send.send(()).await {
            error!("stratum_keepalived: keepalive task ping error: {}", e);
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        return JsonResponse::new(
            JsonValue::Object(HashMap::from([(
                "status".to_string(),
                JsonValue::String("KEEPALIVED".to_string()),
            )])),
            id,
        )
        .into()
    }
}
