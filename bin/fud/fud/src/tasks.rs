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

use std::sync::Arc;

use darkfi::{geode::hash_to_string, system::sleep, Error, Result};

use crate::{
    dht::DhtHandler,
    proto::{FudAnnounce, FudChunkReply, FudFileReply},
    Fud,
};
use log::{error, info};

/// Triggered when calling the `get` RPC method
pub async fn get_task(fud: Arc<Fud>) -> Result<()> {
    loop {
        let (_, file_hash, file_name, _) = fud.get_rx.recv().await.unwrap();

        let _ = fud.handle_get(file_hash, file_name).await;
    }
}

pub enum FetchReply {
    File(FudFileReply),
    Chunk(FudChunkReply),
}

/// Background task that receives file fetch requests and tries to
/// fetch objects from the network using the routing table.
/// TODO: This can be optimised a lot for connection reuse, etc.
pub async fn fetch_file_task(fud: Arc<Fud>) -> Result<()> {
    info!(target: "fud::fetch_file_task()", "Started background file fetch task");
    loop {
        let (file_hash, _) = fud.file_fetch_rx.recv().await.unwrap();
        info!(target: "fud::fetch_file_task()", "Fetching file {}", hash_to_string(&file_hash));

        let result = fud.fetch_file_metadata(file_hash).await;

        match result {
            Some(reply) => {
                match reply {
                    FetchReply::File(FudFileReply { chunk_hashes }) => {
                        if let Err(e) = fud.geode.insert_file(&file_hash, &chunk_hashes).await {
                            error!(
                                "Failed inserting file {} to Geode: {}",
                                hash_to_string(&file_hash),
                                e
                            );
                        }
                        fud.file_fetch_end_tx.send((file_hash, Ok(()))).await.unwrap();
                    }
                    // Looked for a file but got a chunk: the entire file fits in a single chunk
                    FetchReply::Chunk(FudChunkReply { chunk }) => {
                        info!(target: "fud::fetch_file_task()", "File fits in a single chunk");
                        let chunk_hash = blake3::hash(&chunk);
                        let _ = fud.geode.insert_file(&file_hash, &[chunk_hash]).await;
                        match fud.geode.insert_chunk(&chunk).await {
                            Ok(_) => {}
                            Err(e) => {
                                error!(
                                    "Failed inserting chunk {} to Geode: {}",
                                    hash_to_string(&file_hash),
                                    e
                                );
                            }
                        };
                        fud.file_fetch_end_tx.send((file_hash, Ok(()))).await.unwrap();
                    }
                }
            }
            None => {
                fud.file_fetch_end_tx
                    .send((file_hash, Err(Error::GeodeFileRouteNotFound)))
                    .await
                    .unwrap();
            }
        };
    }
}

/// Background task that removes seeders that did not announce a file/chunk
/// for more than an hour.
pub async fn prune_seeders_task(fud: Arc<Fud>) -> Result<()> {
    loop {
        sleep(1800).await; // TODO: Make a setting

        info!(target: "fud::prune_seeders_task()", "Pruning seeders...");
        fud.dht().prune_router(fud.seeders_router.clone(), 3600).await;
    }
}

/// Background task that announces our files and chunks once every hour.
pub async fn announce_seed_task(fud: Arc<Fud>) -> Result<()> {
    loop {
        sleep(3600).await; // TODO: Make a setting

        let seeders = vec![fud.dht().node.clone().into()];

        info!(target: "fud::announce_task()", "Announcing files...");
        let file_hashes = fud.geode.list_files().await;
        if let Ok(files) = file_hashes {
            for file in files {
                let _ = fud
                    .announce(
                        &file,
                        &FudAnnounce { key: file, seeders: seeders.clone() },
                        fud.seeders_router.clone(),
                    )
                    .await;
            }
        }
    }
}
