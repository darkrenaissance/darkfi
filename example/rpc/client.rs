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
use serde_json::json;
use smol::Executor;
use url::Url;

use darkfi::{
    rpc::{client::RpcClient, jsonrpc::JsonRequest},
    Result,
};

async fn realmain(ex: Arc<Executor<'_>>) -> Result<()> {
    let endpoint = Url::parse("tcp://127.0.0.1:55422").unwrap();

    let client = RpcClient::new(endpoint, Some(ex)).await?;

    let req = JsonRequest::new("ping", json!([]));
    let rep = client.request(req).await?;

    println!("{:#?}", rep);

    let req = JsonRequest::new("kill", json!([]));
    let rep = client.request(req).await?;

    println!("{:#?}", rep);

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
