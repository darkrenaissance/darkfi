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

//! Chunk-based file storage implementation.
//! This is a building block for a DHT or something similar.
//!
//! The API supports file insertion and retrieval. There is intentionally no
//! `remove` support. File removal should be handled externally, and then it
//! is only required to run `garbage_collect()` to clean things up.
//!
//! The filesystem hierarchy stores two directories: `files` and `chunks`.
//! `chunks` store [`MAX_CHUNK_SIZE`] files, where the filename is a BLAKE3
//! hash of the chunk's contents.
//! `files` store metadata about a full file, which can be retrieved by
//! concatenating the chunks in order. The filename of a file in `files`
//! is the BLAKE3 hash of hashed chunks in the correct order.
//!
//! It might look like the following:
//! ```
//! /files/7d4c0d5539057c8f9b60d32b423964beb38ecd8ea1ab203c0207990cbf0cad22
//! /files/...
//! /chunks/9d7abc2efa52b8be63ff82b756edb6822e09aa40fc587aba977185a5bb449c19
//! /chunks/fc432e087d16d8788e87640511e627be34a4a50533f1e5ed3e1370645a0266b8
//! /chunks/...
//! ```
//!
//! In the above example, contents of `7d4c0d5539057c8f9b60d32b423964beb38ecd8ea1ab203c0207990cbf0cad22`
//! may be:
//! ```
//! 9d7abc2efa52b8be63ff82b756edb6822e09aa40fc587aba977185a5bb449c19
//! fc432e087d16d8788e87640511e627be34a4a50533f1e5ed3e1370645a0266b8
//! ```
//!
//! This means, in order to retrieve `7d4c0d5539057c8f9b60d32b423964beb38ecd8ea1ab203c0207990cbf0cad22`,
//! we need to concatenate the files under `/chunks` whose filenames are the
//! hashes found above. The contents of the files in `/chunks` are arbitrary
//! data, and by concatenating them we can retrieve the original file.
//!
//! It is important to note that multiple files can use the same chunks.
//! This is some kind of naive deduplication, so we actually don't consider
//! chunks to be specific to a single file and therefore when we do garbage
//! collection, we keep chunks and files independent of each other.

use std::{collections::HashSet, path::PathBuf};

use futures::AsyncRead;
use log::{debug, info, warn};
use smol::{
    fs,
    fs::{File, OpenOptions},
    io::{AsyncBufReadExt, AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufReader, Cursor, SeekFrom},
    stream::StreamExt,
};

use crate::{Error, Result};

/// Defined maximum size of a stored chunk (256 KiB)
pub const MAX_CHUNK_SIZE: usize = 262_144;

/// Path prefix where file metadata is stored
const FILES_PATH: &str = "files";
/// Path prefix where file chunks are stored
const CHUNKS_PATH: &str = "chunks";

/// `ChunkedFile` is a representation of a file we're trying to
/// retrieve from `Geode`.
///
/// The tuple contains `blake3::Hash` of
/// the file's chunks and an optional `PathBuf` which points to
/// the filesystem where the chunk can be found. If `None`, it
/// is to be assumed that the chunk is not available locally.
#[derive(Clone)]
pub struct ChunkedFile(Vec<(blake3::Hash, Option<PathBuf>)>);

impl ChunkedFile {
    fn new(hashes: &[blake3::Hash]) -> Self {
        Self(hashes.iter().map(|x| (*x, None)).collect())
    }

    /// Check whether we have all the chunks available locally.
    pub fn is_complete(&self) -> bool {
        !self.0.iter().any(|(_, p)| p.is_none())
    }

    /// Return an iterator over the chunks and their paths.
    pub fn iter(&self) -> core::slice::Iter<'_, (blake3::Hash, Option<PathBuf>)> {
        self.0.iter()
    }
}

/// Chunk-based file storage interface.
pub struct Geode {
    /// Path to the filesystem directory where file metadata is stored
    files_path: PathBuf,
    /// Path to the filesystem directory where file chunks are stored
    chunks_path: PathBuf,
}

impl Geode {
    /// Instantiate a new [`Geode`] object.
    /// `base_path` defines the root directory where Geode will store its
    /// file metadata and chunks.
    pub async fn new(base_path: &PathBuf) -> Result<Self> {
        let mut files_path: PathBuf = base_path.into();
        let mut chunks_path: PathBuf = base_path.into();
        files_path.push(FILES_PATH);
        chunks_path.push(CHUNKS_PATH);

        // Create necessary directory structure if needed
        fs::create_dir_all(&files_path).await?;
        fs::create_dir_all(&chunks_path).await?;

        Ok(Self { files_path, chunks_path })
    }

    /// Attempt to read chunk hashes from a given file path and return
    /// a `Vec` containing the hashes in order.
    async fn read_metadata(path: &PathBuf) -> Result<Vec<blake3::Hash>> {
        debug!(target: "geode::read_metadata()", "Reading chunks from {:?}", path);
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

    /// Perform garbage collection over the filesystem hierarchy.
    /// Returns sets representing deleted files and deleted chunks, respectively.
    pub async fn garbage_collect(&self) -> Result<(HashSet<blake3::Hash>, HashSet<blake3::Hash>)> {
        info!(target: "geode::garbage_collect()", "[Geode] Performing garbage collection");
        // We track corrupt files and chunks here.
        let mut deleted_files = HashSet::new();
        let mut deleted_chunks = HashSet::new();
        let mut deleted_chunk_paths = HashSet::new();

        // Scan through available chunks and check them for consistency.
        let mut chunk_paths = fs::read_dir(&self.chunks_path).await?;
        let mut buf = [0u8; MAX_CHUNK_SIZE];
        while let Some(chunk) = chunk_paths.next().await {
            let Ok(entry) = chunk else { continue };
            let chunk_path = entry.path();

            // Skip if we're not a plain file
            if !chunk_path.is_file() {
                continue
            }

            // Make sure that the filename is a BLAKE3 hash
            let file_name = match chunk_path.file_name().and_then(|n| n.to_str()) {
                Some(v) => v,
                None => continue,
            };
            let chunk_hash = match blake3::Hash::from_hex(file_name) {
                Ok(v) => v,
                Err(_) => continue,
            };

            // If there is a problem with opening the file, remove it.
            let Ok(mut chunk_fd) = File::open(&chunk_path).await else {
                deleted_chunk_paths.insert(chunk_path);
                deleted_chunks.insert(chunk_hash);
                continue
            };

            // Perform consistency check
            let Ok(bytes_read) = chunk_fd.read(&mut buf).await else {
                deleted_chunk_paths.insert(chunk_path);
                deleted_chunks.insert(chunk_hash);
                buf = [0u8; MAX_CHUNK_SIZE];
                continue
            };

            let chunk_slice = &buf[..bytes_read];
            let hashed_chunk = blake3::hash(chunk_slice);

            // If the hash doesn't match the filename, remove it.
            if chunk_hash != hashed_chunk {
                deleted_chunk_paths.insert(chunk_path);
                deleted_chunks.insert(chunk_hash);
                buf = [0u8; MAX_CHUNK_SIZE];
                continue
            }

            // Seems legit.
            buf = [0u8; MAX_CHUNK_SIZE];
        }

        for chunk_path in &deleted_chunk_paths {
            if let Err(e) = fs::remove_file(chunk_path).await {
                warn!(
                   target: "geode::garbage_collect()",
                   "[Geode] Garbage collect failed to remove corrupted chunk: {}", e,
                );
            }
        }

        // Perform health check over file metadata. For now we just ensure they
        // have the correct format.
        let mut file_paths = fs::read_dir(&self.files_path).await?;
        while let Some(file) = file_paths.next().await {
            let Ok(entry) = file else { continue };
            let path = entry.path();

            // Skip if we're not a plain file
            if !path.is_file() {
                continue
            }

            // Make sure that the filename is a BLAKE3 hash
            let file_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(v) => v,
                None => continue,
            };
            let file_hash = match blake3::Hash::from_hex(file_name) {
                Ok(v) => v,
                Err(_) => continue,
            };

            // The filename is a BLAKE3 hash. It should contain a newline-separated
            // list of chunks which represent the full file. If that is not the case
            // we will consider it a corrupted file and delete it.
            if Self::read_metadata(&path).await.is_err() {
                if let Err(e) = fs::remove_file(path).await {
                    warn!(
                       target: "geode::garbage_collect()",
                       "[Geode] Garbage collect failed to remove corrupted file: {}", e,
                    );
                }

                deleted_files.insert(file_hash);
                continue
            }
        }

        info!(target: "geode::garbage_collect()", "[Geode] Garbage collection finished");
        Ok((deleted_files, deleted_chunks))
    }

    /// Insert a file into Geode. The function expects any kind of byte stream, which
    /// can either be another file on the filesystem, a buffer, etc.
    /// Returns a tuple of `(blake3::Hash, Vec<blake3::Hash>)` which represents the
    /// file name, and the file's chunks, respectively.
    pub async fn insert(
        &self,
        mut stream: impl AsyncRead + Unpin,
    ) -> Result<(blake3::Hash, Vec<blake3::Hash>)> {
        info!(target: "geode::insert()", "[Geode] Inserting file...");
        let mut file_hasher = blake3::Hasher::new();
        let mut chunk_hashes = vec![];
        let mut buf = [0u8; MAX_CHUNK_SIZE];

        while let Ok(bytes_read) = stream.read(&mut buf).await {
            if bytes_read == 0 {
                break
            }

            let chunk_slice = &buf[..bytes_read];
            let chunk_hash = blake3::hash(chunk_slice);
            file_hasher.update(chunk_slice);
            chunk_hashes.push(chunk_hash);

            // Write the chunk to a file, if necessary. We first perform
            // a consistency check and if things are fine, we don't have
            // to perform a write, which is usually more expensive than
            // reading from disk.
            let mut chunk_path = self.chunks_path.clone();
            chunk_path.push(chunk_hash.to_hex().as_str());
            let mut chunk_fd =
                OpenOptions::new().read(true).write(true).create(true).open(&chunk_path).await?;

            let mut fs_buf = [0u8; MAX_CHUNK_SIZE];
            let fs_bytes_read = chunk_fd.read(&mut fs_buf).await?;
            let fs_chunk_slice = &fs_buf[..fs_bytes_read];
            let fs_chunk_hash = blake3::hash(fs_chunk_slice);

            if fs_chunk_hash != chunk_hash {
                debug!(
                    target: "geode::insert()",
                    "Existing chunk inconsistent or unavailable. Writing chunk to {:?}",
                    chunk_path,
                );
                // Here the chunk is broken, so we'll truncate and write the new one.
                chunk_fd.set_len(0).await?;
                chunk_fd.seek(SeekFrom::Start(0)).await?;
                chunk_fd.write_all(chunk_slice).await?;
            } else {
                debug!(
                    target: "geode::insert()",
                    "Existing chunk consistent. Skipping write to {:?}",
                    chunk_path,
                );
            }

            buf = [0u8; MAX_CHUNK_SIZE];
        }

        // This hash is the file's chunks hashed in order.
        let file_hash = file_hasher.finalize();
        let mut file_path = self.files_path.clone();
        file_path.push(file_hash.to_hex().as_str());

        // We always overwrite the metadata.
        let mut file_fd = File::create(&file_path).await?;
        for ch in &chunk_hashes {
            file_fd.write(format!("{}\n", ch.to_hex().as_str()).as_bytes()).await?;
        }

        Ok((file_hash, chunk_hashes))
    }

    /// Create and insert file metadata into Geode given a list of hashes.
    /// Always overwrites any existing file.
    pub async fn insert_file(
        &self,
        file_hash: &blake3::Hash,
        chunk_hashes: &[blake3::Hash],
    ) -> Result<()> {
        info!(target: "geode::insert_file()", "[Geode] Inserting file metadata");

        let mut file_path = self.files_path.clone();
        file_path.push(file_hash.to_hex().as_str());
        let mut file_fd = File::create(&file_path).await?;

        for ch in chunk_hashes {
            file_fd.write(format!("{}\n", ch.to_hex().as_str()).as_bytes()).await?;
        }

        Ok(())
    }

    /// Create and insert a single chunk into Geode given a stream.
    /// Always overwrites any existing chunk. Returns the chunk hash once inserted.
    pub async fn insert_chunk(&self, stream: impl AsRef<[u8]>) -> Result<blake3::Hash> {
        info!(target: "geode::insert_chunk()", "[Geode] Inserting single chunk");

        let mut cursor = Cursor::new(&stream);
        let mut chunk = [0u8; MAX_CHUNK_SIZE];

        let bytes_read = cursor.read(&mut chunk).await?;
        let chunk_slice = &chunk[..bytes_read];
        let chunk_hash = blake3::hash(chunk_slice);

        let mut chunk_path = self.chunks_path.clone();
        chunk_path.push(chunk_hash.to_hex().as_str());
        let mut chunk_fd = File::create(&chunk_path).await?;
        chunk_fd.write_all(chunk_slice).await?;

        Ok(chunk_hash)
    }

    /// Fetch file metadata from Geode. Returns [`ChunkedFile`] which gives a list
    /// of chunks and optionally file paths to the said chunks. Returns an error if
    /// the read failed in any way (could also be the file does not exist).
    pub async fn get(&self, file_hash: &blake3::Hash) -> Result<ChunkedFile> {
        info!(target: "geode::get()", "[Geode] Getting file chunks for {}...", file_hash);
        let mut file_path = self.files_path.clone();
        file_path.push(file_hash.to_hex().as_str());

        // Try to read the file metadata. If it's corrupt, return an error signalling
        // that garbage collection needs to run.
        let chunk_hashes = match Self::read_metadata(&file_path).await {
            Ok(v) => v,
            Err(e) => {
                return match e {
                    // If the file is not found, return according error.
                    Error::Io(std::io::ErrorKind::NotFound) => Err(Error::GeodeFileNotFound),
                    // Anything else should tell the client to do garbage collection
                    _ => Err(Error::GeodeNeedsGc),
                }
            }
        };

        let mut chunked_file = ChunkedFile::new(&chunk_hashes);

        // Iterate over chunks and find which chunks we have available locally.
        let mut buf = [0u8; MAX_CHUNK_SIZE];
        for (chunk_hash, chunk_path) in chunked_file.0.iter_mut() {
            let mut c_path = self.chunks_path.clone();
            c_path.push(chunk_hash.to_hex().as_str());

            if !c_path.exists() || !c_path.is_file() {
                // TODO: We should be aggressive here and remove the non-file.
                continue
            }

            // Perform chunk consistency check
            let mut chunk_fd = File::open(&c_path).await?;
            let bytes_read = chunk_fd.read(&mut buf).await?;
            let chunk_slice = &buf[..bytes_read];
            let hashed_chunk = blake3::hash(chunk_slice);
            if &hashed_chunk != chunk_hash {
                // The chunk is corrupted/inconsistent. Garbage collection should run.
                buf = [0u8; MAX_CHUNK_SIZE];
                continue
            }

            *chunk_path = Some(c_path);
            buf = [0u8; MAX_CHUNK_SIZE];
        }

        Ok(chunked_file)
    }

    /// Fetch a single chunk from Geode. Returns a `PathBuf` pointing to the chunk
    /// if it is found.
    pub async fn get_chunk(&self, chunk_hash: &blake3::Hash) -> Result<PathBuf> {
        info!(target: "geode::get_chunk()", "[Geode] Getting chunk {}", chunk_hash);
        let mut chunk_path = self.chunks_path.clone();
        chunk_path.push(chunk_hash.to_hex().as_str());

        if !chunk_path.exists() || !chunk_path.is_file() {
            // TODO: We should be aggressive here and remove the non-file.
            return Err(Error::GeodeChunkNotFound)
        }

        // Perform chunk consistency check
        let mut buf = [0u8; MAX_CHUNK_SIZE];
        let mut chunk_fd = File::open(&chunk_path).await?;
        let bytes_read = chunk_fd.read(&mut buf).await?;
        let chunk_slice = &buf[..bytes_read];
        let hashed_chunk = blake3::hash(chunk_slice);
        if &hashed_chunk != chunk_hash {
            // The chunk is corrupted
            return Err(Error::GeodeNeedsGc)
        }

        Ok(chunk_path)
    }
}
