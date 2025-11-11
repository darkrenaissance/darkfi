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

use std::path::PathBuf;
use tinyjson::JsonValue;

use darkfi::{
    geode::hash_to_string,
    rpc::util::{json_map, json_str},
};

use crate::{dht::FudSeeder, resource::Resource};

#[derive(Clone, Debug)]
pub struct DownloadStarted {
    pub hash: blake3::Hash,
    pub resource: Resource,
}
#[derive(Clone, Debug)]
pub struct ChunkDownloadCompleted {
    pub hash: blake3::Hash,
    pub chunk_hash: blake3::Hash,
    pub resource: Resource,
}
#[derive(Clone, Debug)]
pub struct MetadataDownloadCompleted {
    pub hash: blake3::Hash,
    pub resource: Resource,
}
#[derive(Clone, Debug)]
pub struct DownloadCompleted {
    pub hash: blake3::Hash,
    pub resource: Resource,
}
#[derive(Clone, Debug)]
pub struct ResourceUpdated {
    pub hash: blake3::Hash,
    pub resource: Resource,
}
#[derive(Clone, Debug)]
pub struct ResourceRemoved {
    pub hash: blake3::Hash,
}
#[derive(Clone, Debug)]
pub struct ChunkNotFound {
    pub hash: blake3::Hash,
    pub chunk_hash: blake3::Hash,
}
#[derive(Clone, Debug)]
pub struct MetadataNotFound {
    pub hash: blake3::Hash,
    pub resource: Resource,
}
#[derive(Clone, Debug)]
pub struct MissingChunks {
    pub hash: blake3::Hash,
    pub resource: Resource,
}
#[derive(Clone, Debug)]
pub struct DownloadError {
    pub hash: blake3::Hash,
    pub error: String,
}
#[derive(Clone, Debug)]
pub struct InsertCompleted {
    pub hash: blake3::Hash,
    pub path: PathBuf,
}
#[derive(Clone, Debug)]
pub struct InsertError {
    pub path: PathBuf,
    pub error: String,
}
#[derive(Clone, Debug)]
pub struct SeedersFound {
    pub hash: blake3::Hash,
    pub seeders: Vec<FudSeeder>,
}

#[derive(Clone, Debug)]
pub enum FudEvent {
    DownloadStarted(DownloadStarted),
    ChunkDownloadCompleted(ChunkDownloadCompleted),
    MetadataDownloadCompleted(MetadataDownloadCompleted),
    DownloadCompleted(DownloadCompleted),
    ResourceUpdated(ResourceUpdated),
    ResourceRemoved(ResourceRemoved),
    ChunkNotFound(ChunkNotFound),
    MetadataNotFound(MetadataNotFound),
    MissingChunks(MissingChunks),
    DownloadError(DownloadError),
    InsertCompleted(InsertCompleted),
    InsertError(InsertError),
    SeedersFound(SeedersFound),
}

impl From<DownloadStarted> for JsonValue {
    fn from(info: DownloadStarted) -> JsonValue {
        json_map([
            ("hash", JsonValue::String(hash_to_string(&info.hash))),
            ("resource", info.resource.into()),
        ])
    }
}
impl From<ChunkDownloadCompleted> for JsonValue {
    fn from(info: ChunkDownloadCompleted) -> JsonValue {
        json_map([
            ("hash", JsonValue::String(hash_to_string(&info.hash))),
            ("chunk_hash", JsonValue::String(hash_to_string(&info.chunk_hash))),
            ("resource", info.resource.into()),
        ])
    }
}
impl From<MetadataDownloadCompleted> for JsonValue {
    fn from(info: MetadataDownloadCompleted) -> JsonValue {
        json_map([
            ("hash", JsonValue::String(hash_to_string(&info.hash))),
            ("resource", info.resource.into()),
        ])
    }
}
impl From<DownloadCompleted> for JsonValue {
    fn from(info: DownloadCompleted) -> JsonValue {
        json_map([
            ("hash", JsonValue::String(hash_to_string(&info.hash))),
            ("resource", info.resource.into()),
        ])
    }
}
impl From<ResourceUpdated> for JsonValue {
    fn from(info: ResourceUpdated) -> JsonValue {
        json_map([
            ("hash", JsonValue::String(hash_to_string(&info.hash))),
            ("resource", info.resource.into()),
        ])
    }
}
impl From<ResourceRemoved> for JsonValue {
    fn from(info: ResourceRemoved) -> JsonValue {
        json_map([("hash", JsonValue::String(hash_to_string(&info.hash)))])
    }
}
impl From<ChunkNotFound> for JsonValue {
    fn from(info: ChunkNotFound) -> JsonValue {
        json_map([
            ("hash", JsonValue::String(hash_to_string(&info.hash))),
            ("chunk_hash", JsonValue::String(hash_to_string(&info.chunk_hash))),
        ])
    }
}
impl From<MetadataNotFound> for JsonValue {
    fn from(info: MetadataNotFound) -> JsonValue {
        json_map([
            ("hash", JsonValue::String(hash_to_string(&info.hash))),
            ("resource", info.resource.into()),
        ])
    }
}
impl From<MissingChunks> for JsonValue {
    fn from(info: MissingChunks) -> JsonValue {
        json_map([
            ("hash", JsonValue::String(hash_to_string(&info.hash))),
            ("resource", info.resource.into()),
        ])
    }
}
impl From<DownloadError> for JsonValue {
    fn from(info: DownloadError) -> JsonValue {
        json_map([
            ("hash", JsonValue::String(hash_to_string(&info.hash))),
            ("error", JsonValue::String(info.error)),
        ])
    }
}
impl From<InsertCompleted> for JsonValue {
    fn from(info: InsertCompleted) -> JsonValue {
        json_map([
            ("hash", JsonValue::String(hash_to_string(&info.hash))),
            ("path", JsonValue::String(info.path.to_string_lossy().to_string())),
        ])
    }
}
impl From<InsertError> for JsonValue {
    fn from(info: InsertError) -> JsonValue {
        json_map([
            ("path", JsonValue::String(info.path.to_string_lossy().to_string())),
            ("error", JsonValue::String(info.error)),
        ])
    }
}
impl From<SeedersFound> for JsonValue {
    fn from(info: SeedersFound) -> JsonValue {
        json_map([
            ("hash", JsonValue::String(hash_to_string(&info.hash))),
            (
                "seeders",
                JsonValue::Array(info.seeders.iter().map(|seeder| seeder.clone().into()).collect()),
            ),
        ])
    }
}
impl From<FudEvent> for JsonValue {
    fn from(event: FudEvent) -> JsonValue {
        match event {
            FudEvent::DownloadStarted(info) => {
                json_map([("event", json_str("download_started")), ("info", info.into())])
            }
            FudEvent::ChunkDownloadCompleted(info) => {
                json_map([("event", json_str("chunk_download_completed")), ("info", info.into())])
            }
            FudEvent::MetadataDownloadCompleted(info) => json_map([
                ("event", json_str("metadata_download_completed")),
                ("info", info.into()),
            ]),
            FudEvent::DownloadCompleted(info) => {
                json_map([("event", json_str("download_completed")), ("info", info.into())])
            }
            FudEvent::ResourceUpdated(info) => {
                json_map([("event", json_str("resource_updated")), ("info", info.into())])
            }
            FudEvent::ResourceRemoved(info) => {
                json_map([("event", json_str("resource_removed")), ("info", info.into())])
            }
            FudEvent::ChunkNotFound(info) => {
                json_map([("event", json_str("chunk_not_found")), ("info", info.into())])
            }
            FudEvent::MetadataNotFound(info) => {
                json_map([("event", json_str("metadata_not_found")), ("info", info.into())])
            }
            FudEvent::MissingChunks(info) => {
                json_map([("event", json_str("missing_chunks")), ("info", info.into())])
            }
            FudEvent::DownloadError(info) => {
                json_map([("event", json_str("download_error")), ("info", info.into())])
            }
            FudEvent::InsertCompleted(info) => {
                json_map([("event", json_str("insert_completed")), ("info", info.into())])
            }
            FudEvent::InsertError(info) => {
                json_map([("event", json_str("insert_error")), ("info", info.into())])
            }
            FudEvent::SeedersFound(info) => {
                json_map([("event", json_str("seeders_found")), ("info", info.into())])
            }
        }
    }
}

/// Macro calling `fud.event_publisher.notify()`
macro_rules! notify_event {
    // This is for any `FudEvent`
    ($fud:expr, $event:ident, { $($fields:tt)* }) => {
        $fud
            .event_publisher
            .notify(FudEvent::$event(event::$event {
                $($fields)*
            }))
            .await;
    };
    // This is for `FudEvent`s that only have a hash and resource
    ($fud:expr, $event:ident, $resource:expr) => {
        $fud
            .event_publisher
            .notify(FudEvent::$event(event::$event {
                hash: $resource.hash,
                resource: $resource.clone(),
            }))
            .await;
    };
}
pub(crate) use notify_event;
