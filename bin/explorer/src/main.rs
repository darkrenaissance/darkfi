/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use std::{
    collections::HashSet,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use async_trait::async_trait;
use darkfi::{
    blockchain::BlockInfo,
    rpc::{
        client::RpcClient,
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResult},
        server::{listen_and_serve, RequestHandler},
        settings::RpcSettings,
    },
    system::{CondVar, Publisher, PublisherPtr, StoppableTask, StoppableTaskPtr},
    util::encoding::base64,
    Error, Result,
};
use darkfi_serial::deserialize_async;
use smol::{
    future,
    lock::{Mutex, MutexGuard},
    Executor,
};
use tapes::{BlobTape, FixedSizedTape, TapeOpenOptions, Tapes};
use tinyjson::JsonValue;
use tracing::{debug, info};

/// Database interfaces
mod db;
use db::{BlockIndex, DifficultyIndex, TxIndex};
/// JSON-RPC server methods
mod rpc;

const RPC_ENDPOINT: &str = "tcp://127.0.0.1:18345";

pub struct Explorer {
    synced: AtomicBool,
    synced_notifier: Arc<CondVar>,
    _sled_db: sled::Db,
    header_indices: sled::Tree,
    tx_indices: sled::Tree,

    tapes_db: Tapes,
    _tapes_options: TapeOpenOptions,
    block_index: FixedSizedTape<BlockIndex>,
    tx_index: FixedSizedTape<TxIndex>,
    difficulty_index: FixedSizedTape<DifficultyIndex>,
    blocks: BlobTape,
    transactions: BlobTape,

    rpc_sub: StoppableTaskPtr,
    rpc_sub_handler: StoppableTaskPtr,
    blocks_publisher: PublisherPtr<JsonResult>,

    rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
}

struct RpcHandler;

#[async_trait]
impl RequestHandler<RpcHandler> for Explorer {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        debug!(target: "explorer::rpc", "--> {}", req.stringify().unwrap());

        match req.method.as_str() {
            "current_difficulty" => self.rpc_current_difficulty(req.id, req.params).await,
            "current_height" => self.rpc_current_height(req.id, req.params).await,
            "latest_blocks" => self.rpc_latest_blocks(req.id, req.params).await,
            "get_block" => self.rpc_get_block(req.id, req.params).await,
            "get_tx" => self.rpc_get_tx(req.id, req.params).await,
            _ => JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
        }
    }

    async fn connections_mut(&self) -> MutexGuard<'life0, HashSet<StoppableTaskPtr>> {
        self.rpc_connections.lock().await
    }
}

impl Explorer {
    fn new(sled_path: &Path, tapes_db_path: &Path, tapes_path: &Path) -> Result<Self> {
        info!("Opening sled dbs");
        let sled_db = sled::open(sled_path)?;
        let header_indices = sled_db.open_tree("header_indices")?;
        let tx_indices = sled_db.open_tree("tx_indices")?;

        info!("Opening tapes dbs");
        std::fs::create_dir_all(tapes_db_path)?;
        std::fs::create_dir_all(tapes_path)?;
        let tapes_db = Tapes::open(tapes_db_path)?;
        let tapes_options =
            TapeOpenOptions { top_cache_size: 64 * 1024, dir: tapes_path.to_path_buf() };

        let (block_index, tx_index, difficulty_index, blocks, transactions) =
            Self::open_tapes(&tapes_db, &tapes_options)?;

        Ok(Self {
            synced: AtomicBool::new(false),
            synced_notifier: Arc::new(CondVar::new()),
            _sled_db: sled_db,
            header_indices,
            tx_indices,
            tapes_db,
            _tapes_options: tapes_options,
            block_index,
            tx_index,
            difficulty_index,
            blocks,
            transactions,
            rpc_sub: StoppableTask::new(),
            rpc_sub_handler: StoppableTask::new(),
            blocks_publisher: Publisher::new(),
            rpc_connections: Mutex::new(HashSet::new()),
        })
    }

    async fn handle_block_sub(&self, ex: Arc<Executor<'_>>) -> Result<()> {
        info!("Started handle_block_sub(), waiting until blockchain is synced");
        let block_subscription = self.blocks_publisher.clone().subscribe().await;
        self.synced_notifier.wait();
        info!("Blockchain synced, processing new blocks...");

        loop {
            // Handle the new block. We get a JsonResult, so also handle
            // any errors that might arise.
            let block_notification = block_subscription.receive().await;
            info!("Got new block notification! {:?}", block_notification);

            match block_notification {
                JsonResult::Notification(notification) => {
                    // TODO: Check if height is lower than our known height.
                    // This means we need to reorg.

                    // Deserialize base64 block
                    let block_bytes =
                        base64::decode(notification.params[0].get::<String>().unwrap()).unwrap();
                    let block: BlockInfo = deserialize_async(&block_bytes).await.unwrap();

                    // Get difficulty
                    let rpc_client =
                        RpcClient::new(RPC_ENDPOINT.parse().unwrap(), ex.clone()).await.unwrap();

                    let req = JsonRequest::new(
                        "blockchain.get_difficulty",
                        JsonValue::Array(vec![(block.header.height as f64).into()]),
                    );
                    let rep = rpc_client.request(req).await?;
                    rpc_client.stop().await;

                    let params = rep.get::<Vec<JsonValue>>().unwrap();
                    let difficulty = *params[0].get::<f64>().unwrap() as u64;
                    let cumulative = *params[1].get::<f64>().unwrap() as u64;

                    let diff = DifficultyIndex { difficulty, cumulative };

                    self.append_block(&block, &diff).await.unwrap();
                    info!("Appended block {}", block.header.height);
                }
                _ => panic!("fixme"),
            }
        }
    }

    async fn sync_blockchain(
        &self,
        from_height: u64,
        to_height: u64,
        ex: Arc<Executor<'_>>,
    ) -> Result<()> {
        if from_height == to_height {
            info!("Blockchain already synced");
            return Ok(())
        }

        info!("sync_blockchain started from_height={from_height} to_height={to_height}...");
        let rpc_client = Arc::new(RpcClient::new(RPC_ENDPOINT.parse().unwrap(), ex.clone()).await?);

        for height in from_height..=to_height {
            info!("Requesting block at height {height}");

            // Get block
            let req = JsonRequest::new(
                "blockchain.get_block",
                JsonValue::Array(vec![(height as f64).into()]),
            );
            let rep = rpc_client.request(req).await?;

            let param = rep.get::<String>().unwrap();
            let bytes = base64::decode(param).unwrap();
            let block: BlockInfo = deserialize_async(&bytes).await?;

            // Get difficulty
            let req = JsonRequest::new(
                "blockchain.get_difficulty",
                JsonValue::Array(vec![(height as f64).into()]),
            );
            let rep = rpc_client.request(req).await?;

            let params = rep.get::<Vec<JsonValue>>().unwrap();
            let difficulty = *params[0].get::<f64>().unwrap() as u64;
            let cumulative = *params[1].get::<f64>().unwrap() as u64;

            let diff = DifficultyIndex { difficulty, cumulative };
            self.append_block(&block, &diff).await?;
        }

        rpc_client.stop().await;
        Ok(())
    }
}

async fn realmain(ex: Arc<Executor<'static>>) -> Result<()> {
    let explorer = Arc::new(Explorer::new(
        Path::new("sled_db"),
        Path::new("tapes_metadata"),
        Path::new("tapes"),
    )?);

    // First we should subscribe to new blocks and queue them to apply
    // after we sync. For this we create a new longterm background task
    // that will handle incoming blocks. It will wait until the blockchain
    // is synced and then proceed to process them.
    let explorer_ = Arc::clone(&explorer);
    let explorer__ = Arc::clone(&explorer);
    let ex_ = ex.clone();
    explorer.rpc_sub_handler.clone().start(
        async move { explorer_.handle_block_sub(ex_).await },
        |res| async move {
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) | Err(Error::RpcServerStopped) => {}
                Err(_) => {
                    explorer__
                        .blocks_publisher
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

    // Then we subscribe to darkfid's RPC to get new blocks. We should first
    // fetch the current height, so we know how far to sync. Then any blocks
    // that come after that should be queued in the `blocks_publisher`.
    let rpc_client = Arc::new(RpcClient::new(RPC_ENDPOINT.parse().unwrap(), ex.clone()).await?);

    let req = JsonRequest::new("blockchain.last_confirmed_block", JsonValue::Array(vec![]));
    let rep = rpc_client.request(req).await?;
    let params = rep.get::<Vec<JsonValue>>().unwrap();
    let last_confirmed_height = *params[0].get::<f64>().unwrap() as u64;

    // Now create the subscription task.
    let rpc_client_ = Arc::clone(&rpc_client);
    let explorer_ = Arc::clone(&explorer);
    let explorer__ = Arc::clone(&explorer);
    explorer.rpc_sub.clone().start(
        async move {
            let req = JsonRequest::new("blockchain.subscribe_blocks", JsonValue::Array(vec![]));
            rpc_client_.subscribe(req, explorer_.blocks_publisher.clone()).await
        },
        |res| async move {
            rpc_client.stop().await;
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) | Err(Error::RpcServerStopped) => {}
                Err(_) => {
                    explorer__
                        .blocks_publisher
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

    // Once the tasks are set up, we'll now perform a manual sync up to
    // the last confirmed height. This will create a new RPC client that
    // is going to request and parse all the necessary blocks, and then
    // apply them to the databases.
    let sync_from = explorer.get_height()?.unwrap_or(0);
    explorer.sync_blockchain(sync_from, last_confirmed_height, ex.clone()).await?;
    explorer.synced.store(true, Ordering::SeqCst);
    explorer.synced_notifier.notify();

    // Start up an RPC server that can be queried for data.
    // This normally serves the data to the Python website frontend.
    info!("Starting JSONRPC server");
    let rpc_settings = RpcSettings::default();
    listen_and_serve(rpc_settings, explorer, None, ex.clone()).await?;

    Ok(())
}

fn main() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = async_channel::unbounded::<()>();

    darkfi::util::logger::setup_logging(1, None)?;

    let (_, result) = easy_parallel::Parallel::new()
        // Run four executor threads
        .each(0..4, |_| future::block_on(ex.run(shutdown.recv())))
        // Run the main future on the current thread
        .finish(|| {
            future::block_on(async {
                realmain(ex.clone()).await?;
                drop(signal);
                Ok::<(), darkfi::Error>(())
            })
        });

    result
}
