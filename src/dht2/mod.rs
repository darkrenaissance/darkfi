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

//! Filesystem-based Distributed Hash-Table (DHT) implementation

use std::collections::HashSet;

use async_std::{
    fs,
    fs::{create_dir_all, File},
    io::{prelude::BufReadExt, BufReader, Cursor, ReadExt, WriteExt},
    path::PathBuf,
    stream::StreamExt,
};
use log::{debug, warn};

use crate::{net::P2pPtr, Error, Result};

/// Networked HashMap
pub mod net_hashmap;
use net_hashmap::NetHashMap;

/// Maximum size of a stored chunk (2 MiB)
pub const MAX_CHUNK_SIZE: usize = 2_097_152;

/// Path prefix where temporary files and concatenated chunks are stored
const TMP_PATH: &str = "tmp";
/// Path prefix where file metadata is stored
const FILES_PATH: &str = "files";
/// Path prefix where file chunks are stored
const CHUNKS_PATH: &str = "chunks";

/// Files distributed on the DHT
pub struct Dht {
    /// Map of hashed files and their (ordered) chunks
    hash_map: NetHashMap<blake3::Hash, Vec<blake3::Hash>>,
    /// Path to the filesystem directory where file metadata is stored
    files_path: PathBuf,
    /// Path to the filesystem directory where the file chunks are stored
    chunks_path: PathBuf,
    /// Path to the filesystem directory where temporary files are stored
    tmp_path: PathBuf,
}

impl Dht {
    /// Instantiate a new [`Dht`] object
    ///
    /// After the object is instantiated, the caller should also run
    /// the [`Dht::garbage_collect()`] function to ensure consistency.
    pub async fn new(base_path: &PathBuf, p2p: P2pPtr) -> Result<Self> {
        let mut tmp_path: PathBuf = base_path.into();
        let mut files_path: PathBuf = base_path.into();
        let mut chunks_path: PathBuf = base_path.into();
        tmp_path.push(TMP_PATH);
        files_path.push(FILES_PATH);
        chunks_path.push(CHUNKS_PATH);

        // Create necessary directory structure if needed
        create_dir_all(&tmp_path).await?;
        create_dir_all(&files_path).await?;
        create_dir_all(&chunks_path).await?;

        Ok(Self { hash_map: NetHashMap::new(p2p), files_path, chunks_path, tmp_path })
    }

    /// Return the `PathBuf` where the file metadata is stored
    pub fn files_path(&self) -> PathBuf {
        self.files_path.clone()
    }

    /// Return the `PathBuf` where the file chunks are stored
    pub fn chunks_path(&self) -> PathBuf {
        self.chunks_path.clone()
    }

    /// Return the `PathBuf` where temporary files are stored
    pub fn tmp_path(&self) -> PathBuf {
        self.tmp_path.clone()
    }

    /// Attempt to read chunk hashes from a given file path
    async fn read_chunks(path: &PathBuf) -> Result<Vec<blake3::Hash>> {
        let fd = File::open(path).await?;
        let mut read_chunks = vec![];
        let mut lines = BufReader::new(fd).lines();
        while let Some(line) = lines.next().await {
            let line = line?;
            let chunk_hash = blake3::Hash::from_hex(line)?;
            read_chunks.push(chunk_hash);
        }

        Ok(read_chunks)
    }

    /// Perform garbage collection over the filesystem hierarchy. This should always
    /// be ran after calling `Dht::new()`.
    pub async fn garbage_collect(&mut self) -> Result<()> {
        debug!(target: "dht", "Performing DHT garbage collection");
        // We track corrupt files and chunks here.
        // After iterating through all files, we will be able to do a cleanup.
        let mut corrupted_files = HashSet::new();
        let mut corrupted_chunks = HashSet::new();

        // Scan through available files and grab the metadata
        let mut file_paths = fs::read_dir(&self.files_path).await?;
        while let Some(file) = file_paths.next().await {
            let Ok(entry) = file else {
                continue
            };

            let path = entry.path();

            // Skip if we're not a plain file
            if !path.is_file().await {
                continue
            }

            // Make sure that the filename is a blake3 hash
            let file_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(v) => v,
                None => continue,
            };

            let file_hash = match blake3::Hash::from_hex(file_name) {
                Ok(v) => v,
                Err(_) => continue,
            };

            // Read the chunk hashes from the file
            let Ok(chunk_hashes) = Self::read_chunks(&path).await else {
                continue
            };

            // Now we have to see that the chunks actually exist and also
            // confirm that hashing all the chunks results in `file_hash`.
            let mut file_hasher = blake3::Hasher::new();
            let mut corrupt_chunks = vec![];
            for chunk_hash in &chunk_hashes {
                let mut buf = vec![0u8; MAX_CHUNK_SIZE];
                let mut chunk_path = self.chunks_path.clone();
                chunk_path.push(chunk_hash.to_hex().as_str());

                let Ok(mut chunk_fd) = File::open(&chunk_path).await else {
                    corrupt_chunks.push(chunk_path.clone());
                    continue
                };

                let Ok(bytes_read) = chunk_fd.read(&mut buf).await else {
                    corrupt_chunks.push(chunk_path.clone());
                    continue
                };

                let chunk_slice = &buf[..bytes_read];
                let hashed_chunk = blake3::hash(chunk_slice);

                if chunk_hash != &hashed_chunk {
                    corrupt_chunks.push(chunk_path.clone());
                    continue
                }

                // Hash the chunk into the file hasher
                file_hasher.update(chunk_slice);
            }

            if !corrupt_chunks.is_empty() {
                for i in corrupt_chunks {
                    corrupted_chunks.insert(i);
                }
            }

            if file_hash != file_hasher.finalize() {
                corrupted_files.insert(path);
                continue
            }

            if !self.hash_map.contains_key(&file_hash) {
                self.hash_map.insert(file_hash, chunk_hashes).await?;
            }
        }

        // At this point we scanned through our hierarchy.
        // Now we can perform a cleanup of corrupted files and chunks.

        // Iterate over what is in the map and find files that use corrupt chunks.
        for (file_hash, file_chunks) in self.hash_map.iter() {
            let mut should_delete = false;

            for chunk in &corrupted_chunks {
                let hash_str = chunk.file_name().unwrap().to_str().unwrap();
                let chunk_hash = blake3::Hash::from_hex(hash_str).unwrap();

                if file_chunks.contains(&chunk_hash) {
                    should_delete = true;
                    break
                }
            }

            if should_delete {
                let mut file_path = self.files_path.clone();
                file_path.push(file_hash.to_hex().as_str());
                corrupted_files.insert(file_path);
            }
        }

        // Now we found all the corrupted files and chunks. Try to delete them.
        for chunk_path in &corrupted_chunks {
            if let Err(e) = fs::remove_file(chunk_path).await {
                warn!(target: "dht", "DHT::garbage_collect(): Failed to remove corrupted chunk: {}", e);
            }
        }

        for file_path in &corrupted_files {
            let hash_str = file_path.file_name().unwrap().to_str().unwrap();
            let file_hash = blake3::Hash::from_hex(hash_str).unwrap();

            self.hash_map.remove(file_hash).await?;

            if let Err(e) = fs::remove_file(file_path).await {
                warn!(target: "dht", "DHT::garbage_collect(): Failed to remove corrupted file: {}", e);
            }
        }

        Ok(())
    }

    /// Attempt to insert a file into the DHT
    pub async fn insert(
        &mut self,
        stream: impl AsRef<[u8]>,
    ) -> Result<(blake3::Hash, Vec<blake3::Hash>)> {
        debug!(target: "dht", "DHT::insert()");
        let mut file_hasher = blake3::Hasher::new();
        let mut chunk_hashes = vec![];

        let mut cursor = Cursor::new(&stream);
        let mut chunk = vec![0u8; MAX_CHUNK_SIZE];

        while let Ok(bytes_read) = cursor.read(&mut chunk).await {
            if bytes_read == 0 {
                break
            }

            let chunk_slice = &chunk[..bytes_read];
            let chunk_hash = blake3::hash(chunk_slice);
            file_hasher.update(chunk_slice);
            chunk_hashes.push(chunk_hash);

            // Write the chunk to a file.
            // TODO: We can avoid writing here if we do a consistency
            //       check and make sure that the chunk on the fs is
            //       not corrupted. Then we can only write as a last
            //       resort, and as a side-effect we fix the corrupted
            //       chunk.
            let mut chunk_path = self.chunks_path.clone();
            chunk_path.push(chunk_hash.to_hex().as_str());
            let mut chunk_fd = File::create(&chunk_path).await?;
            chunk_fd.write_all(chunk_slice).await?;

            chunk = vec![0u8; MAX_CHUNK_SIZE];
        }

        let file_hash = file_hasher.finalize();
        let mut file_path = self.files_path.clone();
        file_path.push(file_hash.to_hex().as_str());

        // Write the metadata
        let mut file_fd = File::create(&file_path).await?;
        for ch in &chunk_hashes {
            file_fd.write(format!("{}\n", ch.to_hex().as_str()).as_bytes()).await?;
        }

        self.hash_map.insert(file_hash, chunk_hashes.clone()).await?;

        Ok((file_hash, chunk_hashes))
    }

    async fn get_file_from_network(&self, _file_hash: &blake3::Hash) -> Result<Vec<blake3::Hash>> {
        todo!()
    }

    /// Attempt to fetch the given set of chunks from the P2P network.
    ///
    /// TODO: This function should attempt to fetch them concurrently.
    async fn get_chunks_from_network(&self, _chunk_hashes: &HashSet<&blake3::Hash>) -> Result<()> {
        Ok(())
    }

    /// Attempt to fetch a chunk from the local storage. Returns a [`PathBuf`] pointing to the file.
    pub async fn get_chunk_local(&self, chunk_hash: &blake3::Hash) -> Result<PathBuf> {
        debug!(target: "dht", "DHT::get_chunk_local()");

        let mut chunk_path = self.chunks_path();
        chunk_path.push(chunk_hash.to_hex().as_str());

        if !chunk_path.exists().await || !chunk_path.is_file().await {
            return Err(Error::DhtChunkNotFound)
        }

        return Ok(chunk_path)
    }

    /// Attempt to fetch the list of chunks for a given file from the local storage.
    /// Returns a `Vec<blake3::Hash>` of the chunks.
    pub async fn get_file_chunk_hashes_local(
        &self,
        file_hash: &blake3::Hash,
    ) -> Result<Vec<blake3::Hash>> {
        debug!(target: "dht", "DHT::get_file_chunk_hashes_local");

        let mut file_path = self.files_path();
        file_path.push(file_hash.to_hex().as_str());

        if !file_path.exists().await || !file_path.is_file().await {
            return Err(Error::DhtFileMetadataNotFound)
        }

        Self::read_chunks(&file_path).await
    }

    /// Attempt to fetch a file from the DHT. Returns a [`PathBuf`] pointing to the file.
    ///
    /// This function will always try to concatenate chunks into a new file.
    /// The reason for this is that even if the filename exists in the tmpdir, we
    /// can still make sure that it's correct and not corrupted by rewriting it.
    pub async fn get(&self, file_hash: &blake3::Hash) -> Result<PathBuf> {
        debug!(target: "dht", "DHT::get()");
        // Try from local metadata/chunks
        let mut file_path = self.files_path.clone();
        file_path.push(file_hash.to_hex().as_str());

        let mut chunk_hashes = Self::read_chunks(&file_path).await;
        if chunk_hashes.is_err() {
            // If we don't have the file locally, fetch it from the network.
            chunk_hashes = self.get_file_from_network(file_hash).await;
        }

        // Bail on any error
        let chunk_hashes = chunk_hashes?;

        // Now we know what the file's chunks are. See if we have them locally
        // and mark any missing ones. The ones we're missing we'll try to fetch
        // from the network.
        let mut missing_chunks = HashSet::new();

        // Find missing chunks
        for chunk_hash in &chunk_hashes {
            let mut chunk_path = self.chunks_path.clone();
            chunk_path.push(chunk_hash.to_hex().as_str());

            if chunk_path.exists().await && chunk_path.is_file().await {
                continue
            }

            if chunk_path.is_dir().await {
                // This is an externally created directory by something else.
                // We can either return an error here or try to remove the directory.
                // Possibly we can design the API in a way that it panics as well,
                // although this is a bit DoS-prone.
                panic!()
            }

            missing_chunks.insert(chunk_hash);
        }

        // Fetch any missing chunks from the P2P network.
        self.get_chunks_from_network(&missing_chunks).await?;

        // At this point we should have all the chunks locally.
        // Let's concatenate them into a file.
        let mut tmp_path = self.tmp_path();
        tmp_path.push(file_hash.to_hex().as_str());
        let mut file_hasher = blake3::Hasher::new();
        let mut tmp_fd = File::create(&tmp_path).await?;

        for chunk_hash in &chunk_hashes {
            let mut buf = vec![0u8; MAX_CHUNK_SIZE];
            let mut chunk_path = self.chunks_path.clone();
            chunk_path.push(chunk_hash.to_hex().as_str());

            let mut chunk_fd = File::open(&chunk_path).await?;
            let bytes_read = chunk_fd.read(&mut buf).await?;

            let chunk_slice = &buf[..bytes_read];
            let hashed_chunk = blake3::hash(chunk_slice);

            if &hashed_chunk != chunk_hash {
                // The chunk is corrupted/inconsistent.
                // TODO: Run garbage collection or notify the user to GC
                // TODO: Also return an error.
                todo!()
            }

            file_hasher.update(chunk_slice);
            tmp_fd.write_all(chunk_slice).await?;
        }

        // If this check fails, it means we got the wrong chunks.
        if file_hash != &file_hasher.finalize() {
            // TODO: Run garbage collection or notify the user to GC
            // TODO: Also return an error.
            todo!()
        }

        Ok(tmp_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{net, net::P2p};
    use rand::{rngs::OsRng, RngCore};

    #[async_std::test]
    async fn dht_local_get_insert() -> Result<()> {
        let mut base_path = std::env::temp_dir();
        base_path.push("dht");

        let settings = net::Settings::default();
        let p2p = P2p::new(settings).await;

        let mut dht = Dht::new(&base_path.clone().into(), p2p).await?;
        dht.garbage_collect().await?;

        let rng = &mut OsRng;
        let mut data = vec![0u8; MAX_CHUNK_SIZE * 3 + 1];
        rng.fill_bytes(&mut data);

        let (file_hash, chunk_hashes) = dht.insert(&data).await?;

        // Check chunk consistency
        let mut file_hasher = blake3::Hasher::new();
        for chunk_hash in &chunk_hashes {
            let mut chunk_path = dht.chunks_path();
            chunk_path.push(chunk_hash.to_hex().as_str());
            let mut buf = vec![0u8; MAX_CHUNK_SIZE];
            let mut fd = File::open(&chunk_path).await?;
            let bytes_read = fd.read(&mut buf).await?;
            let chunk_slice = &buf[..bytes_read];

            assert_eq!(chunk_hash, &blake3::hash(chunk_slice));

            file_hasher.update(chunk_slice);
        }

        // Check file consistency
        assert_eq!(file_hash, file_hasher.finalize());

        let file_path = dht.get(&file_hash).await?;
        let mut read_data = vec![0u8; MAX_CHUNK_SIZE * 3 + 1];
        let mut fd = File::open(&file_path).await?;
        let bytes_read = fd.read(&mut read_data).await?;

        assert_eq!(bytes_read, MAX_CHUNK_SIZE * 3 + 1);
        assert_eq!(data, read_data);

        fs::remove_dir_all(base_path).await?;
        Ok(())
    }
}
