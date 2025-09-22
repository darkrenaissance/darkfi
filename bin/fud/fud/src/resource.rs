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
};
use tinyjson::JsonValue;

use darkfi::{
    geode::{hash_to_string, ChunkedStorage, MAX_CHUNK_SIZE},
    rpc::util::json_map,
    Error, Result,
};

use crate::FileSelection;

#[derive(Clone, Debug)]
pub enum ResourceStatus {
    Downloading,
    Seeding,
    Discovering,
    Incomplete,
    Verifying,
}

impl ResourceStatus {
    pub fn as_str(&self) -> &str {
        match self {
            ResourceStatus::Downloading => "downloading",
            ResourceStatus::Seeding => "seeding",
            ResourceStatus::Discovering => "discovering",
            ResourceStatus::Incomplete => "incomplete",
            ResourceStatus::Verifying => "verifying",
        }
    }
    fn from_str(s: &str) -> Result<Self> {
        match s {
            "downloading" => Ok(ResourceStatus::Downloading),
            "seeding" => Ok(ResourceStatus::Seeding),
            "discovering" => Ok(ResourceStatus::Discovering),
            "incomplete" => Ok(ResourceStatus::Incomplete),
            "verifying" => Ok(ResourceStatus::Verifying),
            _ => Err(Error::Custom("Invalid resource status".to_string())),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ResourceType {
    Unknown,
    File,
    Directory,
}

impl ResourceType {
    pub fn as_str(&self) -> &str {
        match self {
            ResourceType::Unknown => "unknown",
            ResourceType::File => "file",
            ResourceType::Directory => "directory",
        }
    }
    fn from_str(s: &str) -> Result<Self> {
        match s {
            "unknown" => Ok(ResourceType::Unknown),
            "file" => Ok(ResourceType::File),
            "directory" => Ok(ResourceType::Directory),
            _ => Err(Error::Custom("Invalid resource type".to_string())),
        }
    }
}

/// Structure representing the current state of a file or directory on fud.
/// It is used in most `FudEvent`.
#[derive(Clone, Debug)]
pub struct Resource {
    /// Resource hash (used as key in the DHT)
    pub hash: blake3::Hash,
    /// Resource type (file or directory)
    pub rtype: ResourceType,
    /// Path of the resource on the filesystem
    pub path: PathBuf,
    /// Current status of the resource
    pub status: ResourceStatus,
    /// The files the user wants to download
    pub file_selection: FileSelection,

    /// Total number of chunks
    pub total_chunks_count: u64,
    /// Number of chunks we want to download
    pub target_chunks_count: u64,
    /// Number of chunks we already downloaded
    pub total_chunks_downloaded: u64,
    /// Number of chunks we already downloaded,
    /// but only those we want to download on the last fetch request
    pub target_chunks_downloaded: u64,

    /// Total size (in bytes) of the resource
    pub total_bytes_size: u64,
    /// Data (in bytes) we want to download
    pub target_bytes_size: u64,
    /// Data (in bytes) we already downloaded
    pub total_bytes_downloaded: u64,
    /// Data (in bytes) we already downloaded,
    /// but only data we want to download on the last fetch request
    pub target_bytes_downloaded: u64,

    /// Recent speeds in bytes/sec, used to compute the download ETA.
    pub speeds: Vec<f64>,
}

impl Resource {
    pub fn new(
        hash: blake3::Hash,
        rtype: ResourceType,
        path: &Path,
        status: ResourceStatus,
        file_selection: FileSelection,
    ) -> Self {
        Self {
            hash,
            rtype,
            path: path.to_path_buf(),
            status,
            file_selection,
            total_chunks_count: 0,
            target_chunks_count: 0,
            total_chunks_downloaded: 0,
            target_chunks_downloaded: 0,
            total_bytes_size: 0,
            target_bytes_size: 0,
            total_bytes_downloaded: 0,
            target_bytes_downloaded: 0,
            speeds: vec![],
        }
    }

    /// Computes and returns download ETA in seconds using the `speeds` list.
    pub fn get_eta(&self) -> u64 {
        if self.speeds.is_empty() {
            return 0
        }

        let remaining_chunks = self.target_chunks_count - self.target_chunks_downloaded;
        let mean_speed = self.speeds.iter().sum::<f64>() / self.speeds.len() as f64;

        ((remaining_chunks * MAX_CHUNK_SIZE as u64) as f64 / mean_speed) as u64
    }

    /// Returns the list of selected files (absolute paths).
    pub fn get_selected_files(&self, chunked: &ChunkedStorage) -> Vec<PathBuf> {
        match &self.file_selection {
            FileSelection::Set(files) => files
                .iter()
                .map(|file| self.path.join(file))
                .filter(|abs| chunked.get_files().iter().any(|(f, _)| f == abs))
                .collect(),
            FileSelection::All => chunked.get_files().iter().map(|(f, _)| f.clone()).collect(),
        }
    }

    /// Returns the (sub)set of chunk hashes in a ChunkedStorage for a file selection.
    pub fn get_selected_chunks(&self, chunked: &ChunkedStorage) -> HashSet<blake3::Hash> {
        match &self.file_selection {
            FileSelection::Set(files) => {
                let mut chunks = HashSet::new();
                for file in files {
                    chunks.extend(chunked.get_chunks_of_file(&self.path.join(file)));
                }
                chunks
            }
            FileSelection::All => chunked.iter().cloned().map(|(hash, _)| hash).collect(),
        }
    }

    /// Returns the number of bytes we want from a chunk (depends on the file selection).
    pub fn get_selected_bytes(&self, chunked: &ChunkedStorage, chunk: &[u8]) -> usize {
        // If `FileSelection` is not a set, we want all bytes from a chunk
        let file_set = if let FileSelection::Set(files) = &self.file_selection {
            files
        } else {
            return chunk.len();
        };

        let chunk_hash = blake3::hash(chunk);
        let chunk_index = match chunked.iter().position(|(h, _)| *h == chunk_hash) {
            Some(index) => index,
            None => {
                return 0;
            }
        };

        let files = chunked.get_files();
        let chunk_length = chunk.len();
        let position = (chunk_index as u64) * (MAX_CHUNK_SIZE as u64);
        let mut total_selected_bytes = 0;

        // Find the starting file index based on the position
        let mut file_index = 0;
        let mut file_start_pos = 0;

        while file_index < files.len() {
            if file_start_pos + files[file_index].1 > position {
                break;
            }
            file_start_pos += files[file_index].1;
            file_index += 1;
        }

        if file_index >= files.len() {
            // Out of bounds
            return 0;
        }

        // Calculate the end position of the chunk
        let end_position = position + chunk_length as u64;

        // Iterate through the files and count selected bytes
        while file_index < files.len() {
            let (file_path, file_size) = &files[file_index];
            let file_end_pos = file_start_pos + *file_size;

            // Check if the file is in the selection
            if let Ok(rel_file_path) = file_path.strip_prefix(&self.path) {
                if file_set.contains(rel_file_path) {
                    // Calculate the overlap with the chunk
                    let overlap_start = position.max(file_start_pos);
                    let overlap_end = end_position.min(file_end_pos);

                    if overlap_start < overlap_end {
                        total_selected_bytes += (overlap_end - overlap_start) as usize;
                    }
                }
            }

            // Move to the next file
            file_start_pos += *file_size;
            file_index += 1;

            // Stop if we've reached the end of the chunk
            if file_start_pos >= end_position {
                break;
            }
        }

        total_selected_bytes
    }
}

impl From<Resource> for JsonValue {
    fn from(rs: Resource) -> JsonValue {
        json_map([
            ("hash", JsonValue::String(hash_to_string(&rs.hash))),
            ("type", JsonValue::String(rs.rtype.as_str().to_string())),
            (
                "path",
                JsonValue::String(match rs.path.clone().into_os_string().into_string() {
                    Ok(path) => path,
                    Err(_) => "".to_string(),
                }),
            ),
            ("status", JsonValue::String(rs.status.as_str().to_string())),
            ("total_chunks_count", JsonValue::Number(rs.total_chunks_count as f64)),
            ("target_chunks_count", JsonValue::Number(rs.target_chunks_count as f64)),
            ("total_chunks_downloaded", JsonValue::Number(rs.total_chunks_downloaded as f64)),
            ("target_chunks_downloaded", JsonValue::Number(rs.target_chunks_downloaded as f64)),
            ("total_bytes_size", JsonValue::Number(rs.total_bytes_size as f64)),
            ("target_bytes_size", JsonValue::Number(rs.target_bytes_size as f64)),
            ("total_bytes_downloaded", JsonValue::Number(rs.total_bytes_downloaded as f64)),
            ("target_bytes_downloaded", JsonValue::Number(rs.target_bytes_downloaded as f64)),
            ("speeds", JsonValue::Array(rs.speeds.into_iter().map(JsonValue::Number).collect())),
        ])
    }
}

impl From<JsonValue> for Resource {
    fn from(value: JsonValue) -> Self {
        let mut hash_buf = vec![];
        let _ = bs58::decode(value["hash"].get::<String>().unwrap().as_str()).onto(&mut hash_buf);
        let mut hash_buf_arr = [0u8; 32];
        hash_buf_arr.copy_from_slice(&hash_buf);
        let hash = blake3::Hash::from_bytes(hash_buf_arr);

        let rtype = ResourceType::from_str(value["type"].get::<String>().unwrap()).unwrap();
        let path = PathBuf::from(value["path"].get::<String>().unwrap());
        let status = ResourceStatus::from_str(value["status"].get::<String>().unwrap()).unwrap();

        let total_chunks_count = *value["total_chunks_count"].get::<f64>().unwrap() as u64;
        let target_chunks_count = *value["target_chunks_count"].get::<f64>().unwrap() as u64;
        let total_chunks_downloaded =
            *value["total_chunks_downloaded"].get::<f64>().unwrap() as u64;
        let target_chunks_downloaded =
            *value["target_chunks_downloaded"].get::<f64>().unwrap() as u64;
        let total_bytes_size = *value["total_bytes_size"].get::<f64>().unwrap() as u64;
        let target_bytes_size = *value["target_bytes_size"].get::<f64>().unwrap() as u64;
        let total_bytes_downloaded = *value["total_bytes_downloaded"].get::<f64>().unwrap() as u64;
        let target_bytes_downloaded =
            *value["target_bytes_downloaded"].get::<f64>().unwrap() as u64;

        let speeds = value["speeds"]
            .get::<Vec<JsonValue>>()
            .unwrap()
            .iter()
            .map(|s| *s.get::<f64>().unwrap())
            .collect::<Vec<f64>>();

        Resource {
            hash,
            rtype,
            path,
            status,
            file_selection: FileSelection::All, // TODO
            total_chunks_count,
            target_chunks_count,
            total_chunks_downloaded,
            target_chunks_downloaded,
            total_bytes_size,
            target_bytes_size,
            total_bytes_downloaded,
            target_bytes_downloaded,
            speeds,
        }
    }
}
