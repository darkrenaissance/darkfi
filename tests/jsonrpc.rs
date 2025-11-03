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

use std::{collections::HashSet, sync::Arc};

use async_trait::async_trait;
use smol::{
    channel::{Receiver, Sender},
    lock::{Mutex, MutexGuard},
    net::TcpListener,
    Executor,
};
use tinyjson::JsonValue;
use tracing::warn;
use url::Url;

use darkfi::{
    rpc::{
        client::RpcClient,
        jsonrpc::*,
        server::{listen_and_serve, RequestHandler},
        settings::RpcSettings,
    },
    system::{msleep, StoppableTask, StoppableTaskPtr},
    util::logger::{setup_test_logger, Level},
    Error, Result,
};

struct RpcSrv {
    stop_sub: (Sender<()>, Receiver<()>),
    rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
}

impl RpcSrv {
    async fn pong(&self, id: u16, _params: JsonValue) -> JsonResult {
        JsonResponse::new(JsonValue::String("pong".to_string()), id).into()
    }

    async fn kill(&self, id: u16, _params: JsonValue) -> JsonResult {
        self.stop_sub.0.send(()).await.unwrap();
        JsonResponse::new(JsonValue::String("bye".to_string()), id).into()
    }
}

#[async_trait]
impl RequestHandler<()> for RpcSrv {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        assert!(req.params.is_array());

        return match req.method.as_str() {
            "ping" => self.pong(req.id, req.params).await,
            "kill" => self.kill(req.id, req.params).await,
            _ => JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
        }
    }

    async fn connections_mut(&self) -> MutexGuard<'life0, HashSet<StoppableTaskPtr>> {
        self.rpc_connections.lock().await
    }
}

/// Initialize the logging mechanism
fn init_logger() {
    // We check this error so we can execute same file tests in parallel,
    // otherwise second one fails to init logger here.
    if setup_test_logger(
        &[],
        false,
        //Level::Info,
        //Level::Verbose
        Level::Debug,
        //Level::Trace,
    )
    .is_err()
    {
        warn!("Logger already initialized");
    }
}

#[test]
fn jsonrpc_reqrep() -> Result<()> {
    init_logger();
    let executor = Arc::new(Executor::new());

    smol::block_on(executor.run(async {
        // Find an available port
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let sockaddr = listener.local_addr()?;
        let rpc_settings = RpcSettings {
            listen: Url::parse(&format!("tcp://127.0.0.1:{}", sockaddr.port()))?,
            ..RpcSettings::default()
        };
        drop(listener);

        let rpcsrv = Arc::new(RpcSrv {
            stop_sub: smol::channel::unbounded(),
            rpc_connections: Mutex::new(HashSet::new()),
        });
        let rpcsrv_ = Arc::clone(&rpcsrv);

        let rpc_task = StoppableTask::new();
        rpc_task.clone().start(
            listen_and_serve(rpc_settings.clone(), rpcsrv.clone(), None, executor.clone()),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::RpcServerStopped) => rpcsrv_.stop_connections().await,
                    Err(e) => eprintln!("Failed starting JSON-RPC server: {e}"),
                }
            },
            Error::RpcServerStopped,
            executor.clone(),
        );

        msleep(500).await;

        let client = RpcClient::new(rpc_settings.listen, executor.clone()).await?;
        let req = JsonRequest::new("ping", vec![].into());
        let rep = client.request(req).await?;

        let rep = String::try_from(rep).unwrap();
        assert_eq!(&rep, "pong");

        let req = JsonRequest::new("kill", vec![].into());
        let rep = client.request(req).await?;

        let rep = String::try_from(rep).unwrap();
        assert_eq!(&rep, "bye");

        Ok(())
    }))
}

#[test]
fn http_jsonrpc_reqrep() -> Result<()> {
    init_logger();
    let executor = Arc::new(Executor::new());

    smol::block_on(executor.run(async {
        // Find an available port
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let sockaddr = listener.local_addr()?;
        let rpc_settings = RpcSettings {
            listen: Url::parse(&format!("http+tcp://127.0.0.1:{}", sockaddr.port()))?,
            ..RpcSettings::default()
        };
        drop(listener);

        let rpcsrv = Arc::new(RpcSrv {
            stop_sub: smol::channel::unbounded(),
            rpc_connections: Mutex::new(HashSet::new()),
        });
        let rpcsrv_ = Arc::clone(&rpcsrv);

        let rpc_task = StoppableTask::new();
        rpc_task.clone().start(
            listen_and_serve(rpc_settings.clone(), rpcsrv.clone(), None, executor.clone()),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::RpcServerStopped) => rpcsrv_.stop_connections().await,
                    Err(e) => eprintln!("Failed starting JSON-RPC server: {e}"),
                }
            },
            Error::RpcServerStopped,
            executor.clone(),
        );

        msleep(500).await;

        let client = RpcClient::new(rpc_settings.listen, executor.clone()).await?;
        let req = JsonRequest::new("ping", vec![].into());
        let rep = client.request(req).await?;

        let rep = String::try_from(rep).unwrap();
        assert_eq!(&rep, "pong");

        let req = JsonRequest::new("kill", vec![].into());
        let rep = client.request(req).await?;

        let rep = String::try_from(rep).unwrap();
        assert_eq!(&rep, "bye");

        Ok(())
    }))
}
