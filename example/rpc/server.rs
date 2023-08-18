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
use async_std::sync::Arc;
use async_trait::async_trait;
use serde_json::{json, Value};
use smol::{
    channel::{Receiver, Sender},
    Executor,
};
use url::Url;

use darkfi::{
    rpc::{
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResponse, JsonResult},
        server::{listen_and_serve, RequestHandler},
    },
    Result,
};

struct RpcSrv {
    stop_sub: (Sender<()>, Receiver<()>),
}

impl RpcSrv {
    async fn pong(&self, id: Value, _params: &[Value]) -> JsonResult {
        JsonResponse::new(json!("pong"), id).into()
    }

    async fn kill(&self, id: Value, _params: &[Value]) -> JsonResult {
        self.stop_sub.0.send(()).await.unwrap();
        JsonResponse::new(json!("Bye"), id).into()
    }
}

#[async_trait]
impl RequestHandler for RpcSrv {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        if !req.params.is_array() {
            return JsonError::new(ErrorCode::InvalidParams, None, req.id).into()
        }

        let params = req.params.as_array().unwrap();

        match req.method.as_str() {
            Some("ping") => return self.pong(req.id, params).await,
            Some("kill") => return self.kill(req.id, params).await,
            Some(_) | None => return JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
        }
    }
}

async fn realmain(ex: Arc<Executor<'_>>) -> Result<()> {
    let rpcsrv = Arc::new(RpcSrv { stop_sub: smol::channel::unbounded::<()>() });
    //let rpc_listen = Url::parse("tcp://127.0.0.1:55422").unwrap();
    let rpc_listen = Url::parse("unix:///tmp/rpc.sock").unwrap();

    let _ex = ex.clone();
    ex.spawn(listen_and_serve(rpc_listen, rpcsrv.clone(), _ex)).detach();

    rpcsrv.stop_sub.1.recv().await?;

    Ok(())
}

fn main() -> Result<()> {
    simplelog::TermLogger::init(
        simplelog::LevelFilter::Debug,
        simplelog::ConfigBuilder::new().build(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )?;

    let n_threads = std::thread::available_parallelism().unwrap().get();
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();
    let (_, result) = easy_parallel::Parallel::new()
        .each(0..n_threads, |_| smol::future::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::future::block_on(async {
                realmain(ex.clone()).await?;
                drop(signal);
                Ok::<(), darkfi::Error>(())
            })
        });

    result
}
