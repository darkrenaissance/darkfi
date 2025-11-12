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

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::Instant,
};

use futures::{future::FutureExt, pin_mut, select};
use rand::{
    prelude::{IteratorRandom, SliceRandom},
    rngs::OsRng,
};
use tracing::{error, info, warn};

use darkfi::{
    dht::{event::DhtEvent, DhtHandler, DhtNode},
    geode::{hash_to_string, ChunkedStorage},
    net::ChannelPtr,
    system::Subscription,
    Error, Result,
};
use darkfi_serial::serialize_async;

use crate::{
    event::{self, notify_event, FudEvent},
    proto::{
        FudChunkNotFound, FudChunkReply, FudChunkRequest, FudDirectoryReply, FudFileReply,
        FudMetadataNotFound, FudMetadataRequest,
    },
    util::{create_all_files, receive_resource_msg},
    Fud, FudSeeder, ResourceStatus, ResourceType, Scrap,
};

type FudDhtEvent = DhtEvent<<Fud as DhtHandler>::Node, <Fud as DhtHandler>::Value>;

/// Receive seeders from a DHT events subscription, and execute an async
/// expression for each deduplicated seeder once (seeder order is random).
/// It will keep going until the expression returns `Ok(())`, or there are
/// no more seeders.
/// It has an optional `favored_seeder` argument that will be tried first if
/// specified.
macro_rules! seeders_loop {
    ($key:expr, $fud:expr, $dht_sub:expr, $favored_seeder:expr, $code:expr) => {
        let mut queried_seeders: HashSet<blake3::Hash> = HashSet::new();
        let mut is_done = false;

        // Try favored seeder
        let favored_seeder: Option<FudSeeder> = $favored_seeder;
        if let Some(seeder) = favored_seeder {
            queried_seeders.insert(seeder.node.id());
            if $code(seeder).await.is_ok() {
                is_done = true;
            }
        }

        // Try other seeders using the DHT subscription
        while !is_done {
            let event = $dht_sub.receive().await;
            if event.key() != Some($key) {
                continue // Ignore this event if it's not about the right key
            }
            if let DhtEvent::ValueLookupCompleted { .. } = event {
                break // Lookup is done
            }
            if !matches!(event, DhtEvent::ValueFound { .. }) {
                continue // Ignore this event as it's not a ValueFound
            }
            let seeders = event.into_value().unwrap();
            let mut shuffled_seeders = {
                let mut vec: Vec<_> = seeders.iter().cloned().collect();
                vec.shuffle(&mut OsRng);
                vec
            };
            // Loop over seeders
            while let Some(seeder) = shuffled_seeders.pop() {
                // Only use a seeder once
                if queried_seeders.iter().any(|s| *s == seeder.node.id()) {
                    continue
                }
                queried_seeders.insert(seeder.node.id());

                if $code(seeder).await.is_err() {
                    continue
                }

                is_done = true;
                break
            }
        }
    };
    ($key:expr, $fud:expr, $dht_sub:expr, $code:expr) => {
        seeders_loop!($key, $fud, $dht_sub, None, $code)
    };
}

enum ChunkFetchControl {
    NextChunk,
    NextSeeder,
    Abort,
}

struct ChunkFetchContext<'a> {
    fud: &'a Fud,
    hash: &'a blake3::Hash,
    chunked: &'a mut ChunkedStorage,
    chunks: &'a mut HashSet<blake3::Hash>,
}

/// Fetch `chunks` for `chunked` (file or directory) from seeders in `seeders_sub`.
pub async fn fetch_chunks(
    fud: &Fud,
    hash: &blake3::Hash,
    chunked: &mut ChunkedStorage,
    dht_sub: &Subscription<FudDhtEvent>,
    favored_seeder: Option<FudSeeder>,
    chunks: &mut HashSet<blake3::Hash>,
) -> Result<()> {
    let mut ctx = ChunkFetchContext { fud, hash, chunked, chunks };

    seeders_loop!(hash, fud, dht_sub, favored_seeder, async |seeder: FudSeeder| -> Result<()> {
        let (channel, _) = match fud.dht.get_channel(&seeder.node).await {
            Ok(channel) => channel,
            Err(e) => {
                warn!(target: "fud::download::fetch_chunks()", "Could not get a channel for node {}: {e}", hash_to_string(&seeder.node.id()));
                return Err(e)
            }
        };
        let mut chunks_to_query = ctx.chunks.clone();
        info!(target: "fud::download::fetch_chunks()", "Requesting chunks from seeder {}", hash_to_string(&seeder.node.id()));

        loop {
            // Loop over chunks
            match fetch_chunk(&mut ctx, &channel, &seeder, &mut chunks_to_query).await {
                ChunkFetchControl::NextChunk => continue,
                ChunkFetchControl::NextSeeder => break,
                ChunkFetchControl::Abort => {
                    fud.dht.cleanup_channel(channel).await;
                    return Ok(())
                }
            };
        }

        fud.dht.cleanup_channel(channel).await;

        // Stop when there are no missing chunks
        if ctx.chunks.is_empty() {
            return Ok(())
        }

        Err(().into())
    });

    Ok(())
}

/// Fetch a single chunk and return what should be done next
async fn fetch_chunk(
    ctx: &mut ChunkFetchContext<'_>,
    channel: &ChannelPtr,
    seeder: &FudSeeder,
    chunks_to_query: &mut HashSet<blake3::Hash>,
) -> ChunkFetchControl {
    // Select a chunk to request
    let mut chunk = None;
    if let Some(random_chunk) = chunks_to_query.iter().choose(&mut OsRng) {
        chunk = Some(*random_chunk);
    }

    if chunk.is_none() {
        // No more chunks to request from this seeder
        return ChunkFetchControl::NextSeeder;
    }

    let chunk_hash = chunk.unwrap();
    chunks_to_query.remove(&chunk_hash);

    let start_time = Instant::now();
    let msg_subscriber_chunk = channel.subscribe_msg::<FudChunkReply>().await.unwrap();
    let msg_subscriber_notfound = channel.subscribe_msg::<FudChunkNotFound>().await.unwrap();

    let send_res = channel.send(&FudChunkRequest { resource: *ctx.hash, chunk: chunk_hash }).await;
    if let Err(e) = send_res {
        warn!(target: "fud::download::fetch_chunk()", "Error while sending FudChunkRequest: {e}");
        return ChunkFetchControl::NextSeeder;
    }

    let chunk_recv =
        receive_resource_msg(&msg_subscriber_chunk, *ctx.hash, ctx.fud.chunk_timeout).fuse();
    let notfound_recv =
        receive_resource_msg(&msg_subscriber_notfound, *ctx.hash, ctx.fud.chunk_timeout).fuse();

    pin_mut!(chunk_recv, notfound_recv);

    // Wait for a FudChunkReply or FudNotFound
    select! {
        chunk_reply = chunk_recv => {
            msg_subscriber_chunk.unsubscribe().await;
            msg_subscriber_notfound.unsubscribe().await;
            if let Err(e) = chunk_reply {
                warn!(target: "fud::download::fetch_chunk()", "Error waiting for chunk reply: {e}");
                return ChunkFetchControl::NextSeeder;
            }
            let reply = chunk_reply.unwrap();
            handle_chunk_reply(ctx, &chunk_hash, &reply, seeder, &start_time).await
        }
        notfound_reply = notfound_recv => {
            msg_subscriber_chunk.unsubscribe().await;
            msg_subscriber_notfound.unsubscribe().await;
            if let Err(e) = notfound_reply {
                warn!(target: "fud::download::fetch_chunk()", "Error waiting for NOTFOUND reply: {e}");
                return ChunkFetchControl::NextSeeder;
            }
            info!(target: "fud::download::fetch_chunk()", "Received NOTFOUND {} from seeder {}", hash_to_string(&chunk_hash), hash_to_string(&seeder.node.id()));
            notify_event!(ctx.fud, ChunkNotFound, { hash: *ctx.hash, chunk_hash });
            ChunkFetchControl::NextChunk
        }
    }
}

/// Processes an incoming chunk
async fn handle_chunk_reply(
    ctx: &mut ChunkFetchContext<'_>,
    chunk_hash: &blake3::Hash,
    reply: &FudChunkReply,
    seeder: &FudSeeder,
    start_time: &Instant,
) -> ChunkFetchControl {
    let write_res = ctx.fud.geode.write_chunk(ctx.chunked, &reply.chunk).await;
    if let Err(e) = write_res {
        error!(target: "fud::download::handle_chunk_reply()", "Failed inserting chunk {} to Geode: {e}", hash_to_string(chunk_hash));
        return ChunkFetchControl::NextChunk;
    }
    let (inserted_hash, bytes_written) = write_res.unwrap();
    if inserted_hash != *chunk_hash {
        warn!(target: "fud::download::handle_chunk_reply()", "Received chunk does not match requested chunk");
        return ChunkFetchControl::NextChunk;
    }

    info!(target: "fud::download::handle_chunk_reply()", "Received chunk {} from seeder {}", hash_to_string(chunk_hash), hash_to_string(&seeder.node.id()));

    // If we did not write the whole chunk to the filesystem,
    // save the chunk in the scraps.
    if bytes_written < reply.chunk.len() {
        info!(target: "fud::download::handle_chunk_reply()", "Saving chunk {} as a scrap", hash_to_string(chunk_hash));
        let chunk_written = ctx.fud.geode.get_chunk(ctx.chunked, chunk_hash).await;
        if let Err(e) = chunk_written {
            error!(target: "fud::download::handle_chunk_reply()", "Error getting chunk: {e}");
            return ChunkFetchControl::NextChunk;
        }
        let scrap = Scrap {
            chunk: reply.chunk.clone(),
            hash_written: blake3::hash(&chunk_written.unwrap()),
        };
        if let Err(e) =
            ctx.fud.scrap_tree.insert(chunk_hash.as_bytes(), serialize_async(&scrap).await)
        {
            error!(target: "fud::download::handle_chunk_reply()", "Failed to save chunk {} as a scrap: {e}", hash_to_string(chunk_hash));
            return ChunkFetchControl::NextChunk;
        }
    }

    // Update the resource
    let mut resources_write = ctx.fud.resources.write().await;
    let resource = resources_write.get_mut(ctx.hash);
    if resource.is_none() {
        return ChunkFetchControl::Abort // Resource was removed
    }
    let resource = resource.unwrap();
    resource.status = ResourceStatus::Downloading;
    resource.total_chunks_downloaded += 1;
    resource.target_chunks_downloaded += 1;

    resource.total_bytes_downloaded += reply.chunk.len() as u64;
    resource.target_bytes_downloaded +=
        resource.get_selected_bytes(ctx.chunked, &reply.chunk) as u64;
    resource.speeds.push(reply.chunk.len() as f64 / start_time.elapsed().as_secs_f64());
    if resource.speeds.len() > 12 {
        resource.speeds = resource.speeds.split_off(resource.speeds.len() - 12); // Only keep the last few speeds
    }

    // If we just fetched the last chunk of a file, compute
    // `total_bytes_size` (and `target_bytes_size`) again,
    // as `geode.write_chunk()` updated the FileSequence
    // to the exact file size.
    if let Some((last_chunk_hash, _)) = ctx.chunked.iter().last() {
        if matches!(resource.rtype, ResourceType::File) && *last_chunk_hash == *chunk_hash {
            resource.total_bytes_size = ctx.chunked.get_fileseq().len();
            resource.target_bytes_size = resource.total_bytes_size;
        }
    }
    let resource = resource.clone();
    drop(resources_write);

    notify_event!(ctx.fud, ChunkDownloadCompleted, { hash: *ctx.hash, chunk_hash: *chunk_hash, resource });
    ctx.chunks.remove(chunk_hash);
    ChunkFetchControl::NextChunk
}

enum MetadataFetchReply {
    Directory(FudDirectoryReply),
    File(FudFileReply),
    Chunk(FudChunkReply),
}

/// Fetch a single resource metadata from seeders received from `seeders_sub`.
/// If the resource is a file smaller than a single chunk then seeder can send the
/// chunk directly, and we will create the file from it on path `path`.
/// 1. Wait for seeders from the subscription
/// 2. Request the metadata from the seeders
/// 3. Insert metadata to geode using the reply
pub async fn fetch_metadata(
    fud: &Fud,
    hash: &blake3::Hash,
    path: &Path,
    dht_sub: &Subscription<FudDhtEvent>,
) -> Result<FudSeeder> {
    let mut result: Option<(FudSeeder, MetadataFetchReply)> = None;

    seeders_loop!(hash, fud, dht_sub, async |seeder: FudSeeder| -> Result<()> {
        let (channel, _) = fud.dht.get_channel(&seeder.node).await?;
        let msg_subscriber_chunk = channel.subscribe_msg::<FudChunkReply>().await.unwrap();
        let msg_subscriber_file = channel.subscribe_msg::<FudFileReply>().await.unwrap();
        let msg_subscriber_dir = channel.subscribe_msg::<FudDirectoryReply>().await.unwrap();
        let msg_subscriber_notfound = channel.subscribe_msg::<FudMetadataNotFound>().await.unwrap();

        let send_res = channel.send(&FudMetadataRequest { resource: *hash }).await;
        if let Err(e) = send_res {
            warn!(target: "fud::download::fetch_metadata()", "Error while sending FudMetadataRequest: {e}");
            msg_subscriber_chunk.unsubscribe().await;
            msg_subscriber_file.unsubscribe().await;
            msg_subscriber_dir.unsubscribe().await;
            msg_subscriber_notfound.unsubscribe().await;
            fud.dht.cleanup_channel(channel).await;
            return Err(e)
        }

        let chunk_recv =
            receive_resource_msg(&msg_subscriber_chunk, *hash, fud.chunk_timeout).fuse();
        let file_recv = receive_resource_msg(&msg_subscriber_file, *hash, fud.chunk_timeout).fuse();
        let dir_recv = receive_resource_msg(&msg_subscriber_dir, *hash, fud.chunk_timeout).fuse();
        let notfound_recv =
            receive_resource_msg(&msg_subscriber_notfound, *hash, fud.chunk_timeout).fuse();

        pin_mut!(chunk_recv, file_recv, dir_recv, notfound_recv);

        let cleanup = async || {
            msg_subscriber_chunk.unsubscribe().await;
            msg_subscriber_file.unsubscribe().await;
            msg_subscriber_dir.unsubscribe().await;
            msg_subscriber_notfound.unsubscribe().await;
            fud.dht.cleanup_channel(channel).await;
        };

        // Wait for a FudChunkReply, FudFileReply, FudDirectoryReply, or FudNotFound
        select! {
            // Received a chunk while requesting metadata, this is allowed to
            // optimize fetching files smaller than a single chunk
            chunk_reply = chunk_recv => {
                cleanup().await;
                if let Err(e) = chunk_reply {
                    warn!(target: "fud::download::fetch_metadata()", "Error waiting for chunk reply: {e}");
                    return Err(e)
                }
                let reply = chunk_reply.unwrap();
                let chunk_hash = blake3::hash(&reply.chunk);
                // Check that this is the only chunk in the file
                if !fud.geode.verify_metadata(hash, &[chunk_hash], &[]) {
                    warn!(target: "fud::download::fetch_metadata()", "Received a chunk while fetching metadata, but the chunk did not match the file hash");
                    return Err(().into())
                }
                info!(target: "fud::download::fetch_metadata()", "Received chunk {} (for file {}) from seeder {}", hash_to_string(&chunk_hash), hash_to_string(hash), hash_to_string(&seeder.node.id()));
                result = Some((seeder, MetadataFetchReply::Chunk((*reply).clone())));
                Ok(())
            }
            file_reply = file_recv => {
                cleanup().await;
                if let Err(e) = file_reply {
                    warn!(target: "fud::download::fetch_metadata()", "Error waiting for file reply: {e}");
                    return Err(e)
                }
                let reply = file_reply.unwrap();
                if !fud.geode.verify_metadata(hash, &reply.chunk_hashes, &[]) {
                    warn!(target: "fud::download::fetch_metadata()", "Received invalid file metadata");
                    return Err(().into())
                }
                info!(target: "fud::download::fetch_metadata()", "Received file {} from seeder {}", hash_to_string(hash), hash_to_string(&seeder.node.id()));
                result = Some((seeder, MetadataFetchReply::File((*reply).clone())));
                Ok(())
            }
            dir_reply = dir_recv => {
                cleanup().await;
                if let Err(e) = dir_reply {
                    warn!(target: "fud::download::fetch_metadata()", "Error waiting for directory reply: {e}");
                    return Err(e)
                }
                let reply = dir_reply.unwrap();

                // Convert all file paths from String to PathBuf
                let files: Vec<_> = reply.files.clone().into_iter()
                    .map(|(path_str, size)| (PathBuf::from(path_str), size))
                    .collect();

                if !fud.geode.verify_metadata(hash, &reply.chunk_hashes, &files) {
                    warn!(target: "fud::download::fetch_metadata()", "Received invalid directory metadata");
                    return Err(().into())
                }
                info!(target: "fud::download::fetch_metadata()", "Received directory {} from seeder {}", hash_to_string(hash), hash_to_string(&seeder.node.id()));
                result = Some((seeder, MetadataFetchReply::Directory((*reply).clone())));
                Ok(())
            }
            notfound_reply = notfound_recv => {
                cleanup().await;
                if let Err(e) = notfound_reply {
                    warn!(target: "fud::download::fetch_metadata()", "Error waiting for NOTFOUND reply: {e}");
                    return Err(e)
                }
                info!(target: "fud::download::fetch_metadata()", "Received NOTFOUND {} from seeder {}", hash_to_string(hash), hash_to_string(&seeder.node.id()));
                Err(().into())
            }
        }
    });

    // We did not find the resource
    if result.is_none() {
        return Err(Error::GeodeFileRouteNotFound)
    }

    // Insert metadata to geode using the reply
    // At this point the reply content is already verified
    let (seeder, reply) = result.unwrap();
    match reply {
        MetadataFetchReply::Directory(FudDirectoryReply { files, chunk_hashes, .. }) => {
            // Convert all file paths from String to PathBuf
            let mut files: Vec<_> =
                files.into_iter().map(|(path_str, size)| (PathBuf::from(path_str), size)).collect();

            fud.geode.sort_files(&mut files);
            if let Err(e) = fud.geode.insert_metadata(hash, &chunk_hashes, &files).await {
                error!(target: "fud::download::fetch_metadata()", "Failed inserting directory {} to Geode: {e}", hash_to_string(hash));
                return Err(e)
            }
        }
        MetadataFetchReply::File(FudFileReply { chunk_hashes, .. }) => {
            if let Err(e) = fud.geode.insert_metadata(hash, &chunk_hashes, &[]).await {
                error!(target: "fud::download::fetch_metadata()", "Failed inserting file {} to Geode: {e}", hash_to_string(hash));
                return Err(e)
            }
        }
        // Looked for a file but got a chunk: the entire file fits in a single chunk
        MetadataFetchReply::Chunk(FudChunkReply { chunk, .. }) => {
            info!(target: "fud::download::fetch_metadata()", "File fits in a single chunk");
            let chunk_hash = blake3::hash(&chunk);
            if let Err(e) = fud.geode.insert_metadata(hash, &[chunk_hash], &[]).await {
                error!(target: "fud::download::fetch_metadata()", "Failed inserting file {} to Geode (from single chunk): {e}", hash_to_string(hash));
                return Err(e)
            }
            create_all_files(&[path.to_path_buf()]).await?;
            let mut chunked_file = ChunkedStorage::new(
                &[chunk_hash],
                &[(path.to_path_buf(), chunk.len() as u64)],
                false,
            );
            if let Err(e) = fud.geode.write_chunk(&mut chunked_file, &chunk).await {
                error!(target: "fud::download::fetch_metadata()", "Failed inserting chunk {} to Geode: {e}", hash_to_string(&chunk_hash));
                return Err(e)
            };
        }
    };

    Ok(seeder)
}
