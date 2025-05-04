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

use log::{error, info};
use std::sync::Arc;

use darkfi::{dht::DhtHandler, geode::hash_to_string, system::sleep, Error, Result};

use crate::{
    proto::{FudAnnounce, FudChunkReply, FudFileReply},
    Fud,
};

/// Triggered when calling the `get` RPC method
pub async fn get_task(fud: Arc<Fud>) -> Result<()> {
    loop {
        let (_, file_hash, file_path, _) = fud.get_rx.recv().await.unwrap();

        let _ = fud.handle_get(&file_hash, &file_path).await;
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
        let (nodes, file_hash, _) = fud.file_fetch_rx.recv().await.unwrap();
        info!(target: "fud::fetch_file_task()", "Fetching file {}", hash_to_string(&file_hash));

        let result = fud.fetch_file_metadata(nodes, file_hash).await;

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

/// Background task that announces our files and chunks once every hour.
/// Also removes seeders that did not announce for too long.
pub async fn announce_seed_task(fud: Arc<Fud>) -> Result<()> {
    let interval = 3600; // TODO: Make a setting

    loop {
        sleep(interval).await;

        let seeders = vec![fud.dht().node().await.into()];

        info!(target: "fud::announce_task()", "Verifying seeds...");
        let seeding_resources = match fud.get_seeding_resources().await {
            Ok(resources) => resources,
            Err(e) => {
                error!(target: "fud::announce_task()", "Error while verifying seeding resources: {}", e);
                continue;
            }
        };

        info!(target: "fud::announce_task()", "Announcing files...");
        for resource in seeding_resources {
            let _ = fud
                .announce(
                    &resource.hash,
                    &FudAnnounce { key: resource.hash, seeders: seeders.clone() },
                    fud.seeders_router.clone(),
                )
                .await;
        }

        info!(target: "fud::announce_task()", "Pruning seeders...");
        fud.dht().prune_router(fud.seeders_router.clone(), interval.try_into().unwrap()).await;
    }
}
