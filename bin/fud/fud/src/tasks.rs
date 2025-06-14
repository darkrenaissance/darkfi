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
use std::{path::PathBuf, sync::Arc};

use darkfi::{
    dht::DhtHandler,
    geode::{hash_to_string, ChunkedStorage},
    system::sleep,
    Error, Result,
};

use crate::{
    proto::{FudAnnounce, FudChunkReply, FudDirectoryReply, FudFileReply},
    Fud,
};

pub enum FetchReply {
    Directory(FudDirectoryReply),
    File(FudFileReply),
    Chunk(FudChunkReply),
}

/// Triggered when calling the `get` RPC method
pub async fn get_task(fud: Arc<Fud>) -> Result<()> {
    loop {
        let (file_hash, file_path) = fud.get_rx.recv().await.unwrap();

        let _ = fud.get(&file_hash, &file_path).await;
    }
}

/// Background task that receives file fetch requests and tries to
/// fetch objects from the network using the routing table.
/// TODO: This can be optimised a lot for connection reuse, etc.
pub async fn fetch_metadata_task(fud: Arc<Fud>) -> Result<()> {
    info!(target: "fud::fetch_metadata_task()", "Started background metadata fetch task");
    loop {
        let (nodes, hash, path) = fud.metadata_fetch_rx.recv().await.unwrap();
        info!(target: "fud::fetch_metadata_task()", "Fetching metadata for {}", hash_to_string(&hash));

        let reply = fud.fetch_metadata(&nodes, &hash).await;
        if reply.is_none() {
            fud.metadata_fetch_end_tx.send(Err(Error::GeodeFileRouteNotFound)).await.unwrap();
            continue
        }
        let reply = reply.unwrap();

        // At this point the reply content was already verified in `fud.fetch_metadata`
        match reply {
            FetchReply::Directory(FudDirectoryReply { files, chunk_hashes }) => {
                // Convert all file paths from String to PathBuf
                let mut files: Vec<_> = files
                    .into_iter()
                    .map(|(path_str, size)| (PathBuf::from(path_str), size))
                    .collect();

                fud.geode.sort_files(&mut files);
                if let Err(e) = fud.geode.insert_metadata(&hash, &chunk_hashes, &files).await {
                    error!(target: "fud::fetch_metadata_task()", "Failed inserting directory {} to Geode: {}", hash_to_string(&hash), e);
                    fud.metadata_fetch_end_tx.send(Err(e)).await.unwrap();
                    continue
                }
                fud.metadata_fetch_end_tx.send(Ok(())).await.unwrap();
            }
            FetchReply::File(FudFileReply { chunk_hashes }) => {
                if let Err(e) = fud.geode.insert_metadata(&hash, &chunk_hashes, &[]).await {
                    error!(target: "fud::fetch_metadata_task()", "Failed inserting file {} to Geode: {}", hash_to_string(&hash), e);
                    fud.metadata_fetch_end_tx.send(Err(e)).await.unwrap();
                    continue
                }
                fud.metadata_fetch_end_tx.send(Ok(())).await.unwrap();
            }
            // Looked for a file but got a chunk: the entire file fits in a single chunk
            FetchReply::Chunk(FudChunkReply { chunk }) => {
                info!(target: "fud::fetch_metadata_task()", "File fits in a single chunk");
                let chunk_hash = blake3::hash(&chunk);
                let _ = fud.geode.insert_metadata(&hash, &[chunk_hash], &[]).await;
                let mut chunked_file =
                    ChunkedStorage::new(&[chunk_hash], &[(path, chunk.len() as u64)], false);
                if let Err(e) = fud.geode.write_chunk(&mut chunked_file, &chunk).await {
                    error!(target: "fud::fetch_metadata_task()", "Failed inserting chunk {} to Geode: {}", hash_to_string(&chunk_hash), e);
                    fud.metadata_fetch_end_tx.send(Err(e)).await.unwrap();
                    continue
                };
                fud.metadata_fetch_end_tx.send(Ok(())).await.unwrap();
            }
        }
    }
}

/// Background task that announces our files once every hour.
/// Also removes seeders that did not announce for too long.
pub async fn announce_seed_task(fud: Arc<Fud>) -> Result<()> {
    let interval = 3600; // TODO: Make a setting

    loop {
        sleep(interval).await;

        let seeders = vec![fud.dht().node().await.into()];

        info!(target: "fud::announce_task()", "Verifying seeds...");
        let seeding_resources = match fud.verify_resources(None).await {
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
