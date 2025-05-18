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
//! The filesystem hierarchy stores a `files` directory storing metadata
//! about a full file. The filename of a file in `files` is the BLAKE3
//! hash of hashed chunks in the correct order. Inside the file is the list
//! of the chunks making up the full file.
//!
//! To get the chunks you split the full file into `MAX_CHUNK_SIZE` sized
//! slices, where the last chunk is the only one that can be smaller than
//! that.
//!
//! It might look like the following:
//! ```
//! /files/B9fFKaEYphw2oH5PDbeL1TTAcSzL6ax84p8SjBKzuYzX
//! /files/8nA3ndjFFee3n5wMPLZampLpGaMJi3od4MSyaXPDoF91
//! /files/...
//! ```
//!
//! In the above example, contents of `B9fFKaEYphw2oH5PDbeL1TTAcSzL6ax84p8SjBKzuYzX`
//! may be:
//! ```
//! 2bQPxSR8Frz7S7JW3DRAzEtkrHfLXB1CN65V7az77pUp
//! CvjvN6MfWQYK54DgKNR7MPgFSZqsCgpWKF2p8ot66CCP
//! ```
//!
//! This means, the file `B9fFKaEYphw2oH5PDbeL1TTAcSzL6ax84p8SjBKzuYzX`
//! is the concatenation of the chunks with the above hashes.
//!
//! The full file is not copied, and individual chunks are not stored by
//! geode. Additionally it does not keep track of the full files path.

use std::{collections::HashSet, path::PathBuf};

use futures::{AsyncRead, AsyncSeek};
use log::{debug, info, warn};
use smol::{
    fs::{self, File, OpenOptions},
    io::{
        self, AsyncBufReadExt, AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufReader, Cursor,
        SeekFrom,
    },
    stream::StreamExt,
};

use crate::{Error, Result};

/// Defined maximum size of a stored chunk (256 KiB)
pub const MAX_CHUNK_SIZE: usize = 262_144;

/// Path prefix where file metadata is stored
const FILES_PATH: &str = "files";

pub fn hash_to_string(hash: &blake3::Hash) -> String {
    bs58::encode(hash.as_bytes()).into_string()
}

/// `ChunkedFile` is a representation of a file we're trying to
/// retrieve from `Geode`.
///
/// The tuple contains `blake3::Hash` of
/// the file's chunks and an optional `PathBuf` which points to
/// the filesystem where the chunk can be found. If `None`, it
/// is to be assumed that the chunk is not available locally.
#[derive(Clone)]
pub struct ChunkedFile(Vec<(blake3::Hash, Option<bool>)>);

impl ChunkedFile {
    fn new(hashes: &[blake3::Hash]) -> Self {
        Self(hashes.iter().map(|x| (*x, None)).collect())
    }

    /// Check whether we have all the chunks available locally.
    pub fn is_complete(&self) -> bool {
        !self.0.iter().any(|(_, p)| p.is_none())
    }

    /// Return an iterator over the chunks and their paths.
    pub fn iter(&self) -> core::slice::Iter<'_, (blake3::Hash, Option<bool>)> {
        self.0.iter()
    }

    /// Return the number of chunks.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Return `true` if the chunked file contains no chunk.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Return the number of chunks available locally.
    pub fn local_chunks(&self) -> usize {
        self.0.iter().filter(|(_, p)| p.is_some()).count()
    }
}

/// Chunk-based file storage interface.
pub struct Geode {
    /// Path to the filesystem directory where file metadata is stored
    files_path: PathBuf,
}

/// smol::fs::File::read does not guarantee that the buffer will be filled, even if the buffer is
/// smaller than the file. This is a workaround.
/// This reads the stream until the buffer is full or until we reached the end of the stream.
pub async fn read_until_filled(
    mut stream: impl AsyncRead + Unpin,
    buffer: &mut [u8],
) -> io::Result<usize> {
    let mut total_bytes_read = 0;

    while total_bytes_read < buffer.len() {
        let bytes_read = stream.read(&mut buffer[total_bytes_read..]).await?;
        if bytes_read == 0 {
            break; // EOF reached
        }
        total_bytes_read += bytes_read;
    }

    Ok(total_bytes_read)
}

impl Geode {
    /// Instantiate a new [`Geode`] object.
    /// `base_path` defines the root directory where Geode will store its
    /// file metadata and chunks.
    pub async fn new(base_path: &PathBuf) -> Result<Self> {
        let mut files_path: PathBuf = base_path.into();
        files_path.push(FILES_PATH);

        // Create necessary directory structure if needed
        fs::create_dir_all(&files_path).await?;

        Ok(Self { files_path })
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
            let mut hash_buf = [0u8; 32];
            bs58::decode(line).onto(&mut hash_buf)?;
            let chunk_hash = blake3::Hash::from_bytes(hash_buf);
            read_chunks.push(chunk_hash);
        }

        Ok(read_chunks)
    }

    /// Perform garbage collection over the filesystem hierarchy.
    /// Returns a set representing deleted files.
    pub async fn garbage_collect(&self) -> Result<HashSet<blake3::Hash>> {
        info!(target: "geode::garbage_collect()", "[Geode] Performing garbage collection");
        // We track corrupt files here.
        let mut deleted_files = HashSet::new();

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
            let mut hash_buf = [0u8; 32];
            let file_hash = match bs58::decode(file_name).onto(&mut hash_buf) {
                Ok(_) => blake3::Hash::from_bytes(hash_buf),
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
        Ok(deleted_files)
    }

    /// Insert a file into Geode. The function expects any kind of byte stream, which
    /// can either be another file on the filesystem, a buffer, etc.
    /// Returns a tuple of `(blake3::Hash, Vec<blake3::Hash>)` which represents the
    /// file hash, and the file's chunks, respectively.
    pub async fn insert(
        &self,
        mut stream: impl AsyncRead + Unpin,
    ) -> Result<(blake3::Hash, Vec<blake3::Hash>)> {
        info!(target: "geode::insert()", "[Geode] Inserting file...");
        let mut file_hasher = blake3::Hasher::new();
        let mut chunk_hashes = vec![];

        loop {
            let mut buf = [0u8; MAX_CHUNK_SIZE];
            let bytes_read = read_until_filled(&mut stream, &mut buf).await?;
            if bytes_read == 0 {
                break
            }

            let chunk_slice = &buf[..bytes_read];
            let chunk_hash = blake3::hash(chunk_slice);
            file_hasher.update(chunk_hash.as_bytes());
            chunk_hashes.push(chunk_hash);
        }

        // This hash is the file's chunks hashes hashed in order.
        let file_hash = file_hasher.finalize();
        let mut file_path = self.files_path.clone();
        file_path.push(hash_to_string(&file_hash).as_str());

        // We always overwrite the metadata.
        let mut file_fd = File::create(&file_path).await?;
        for ch in &chunk_hashes {
            file_fd.write(format!("{}\n", hash_to_string(ch).as_str()).as_bytes()).await?;
        }

        file_fd.flush().await?;

        Ok((file_hash, chunk_hashes))
    }

    /// Create and insert file metadata into Geode given a list of hashes.
    /// Always overwrites any existing file.
    /// Verifies that the file hash matches the chunk hashes
    pub async fn insert_file(
        &self,
        file_hash: &blake3::Hash,
        chunk_hashes: &[blake3::Hash],
    ) -> Result<()> {
        info!(target: "geode::insert_file()", "[Geode] Inserting file metadata");

        if !self.verify_file(file_hash, chunk_hashes) {
            // The chunk list or file hash is wrong
            return Err(Error::GeodeNeedsGc)
        }

        let mut file_path = self.files_path.clone();
        file_path.push(hash_to_string(file_hash).as_str());
        let mut file_fd = File::create(&file_path).await?;

        for ch in chunk_hashes {
            file_fd.write(format!("{}\n", hash_to_string(ch).as_str()).as_bytes()).await?;
        }
        file_fd.flush().await?;

        Ok(())
    }

    /// Write a single chunk into `file_path` given a stream.
    /// The file must be inserted into Geode before calling this method.
    /// Always overwrites any existing chunk. Returns the chunk hash once inserted.
    pub async fn write_chunk(
        &self,
        file_hash: &blake3::Hash,
        file_path: &PathBuf,
        stream: impl AsRef<[u8]>,
    ) -> Result<blake3::Hash> {
        info!(target: "geode::write_chunk()", "[Geode] Writing single chunk");

        let mut cursor = Cursor::new(&stream);
        let mut chunk = [0u8; MAX_CHUNK_SIZE];

        let bytes_read = read_until_filled(&mut cursor, &mut chunk).await?;
        let chunk_slice = &chunk[..bytes_read];
        let chunk_hash = blake3::hash(chunk_slice);

        let chunked_file = self.get(file_hash, file_path).await?;

        // Get the chunk index in the file from the chunk hash
        let chunk_index = match chunked_file.iter().position(|c| c.0 == chunk_hash) {
            Some(index) => index,
            None => {
                return Err(Error::GeodeNeedsGc);
            }
        };

        let position = (chunk_index as u64) * (MAX_CHUNK_SIZE as u64);

        // Create the file if it does not exist
        if !file_path.exists() {
            File::create(&file_path).await?;
        }

        let mut file_fd = OpenOptions::new().write(true).open(&file_path).await?;
        file_fd.seek(SeekFrom::Start(position)).await?;
        file_fd.write_all(chunk_slice).await?;
        file_fd.flush().await?;

        Ok(chunk_hash)
    }

    /// Fetch file metadata from Geode. Returns [`ChunkedFile`] which gives a list
    /// of chunks and booleans to know if the chunks we have are valid. Returns an error if
    /// the read failed in any way (could also be the file does not exist).
    pub async fn get(&self, file_hash: &blake3::Hash, file_path: &PathBuf) -> Result<ChunkedFile> {
        let file_hash_str = hash_to_string(file_hash);
        info!(target: "geode::get()", "[Geode] Getting file chunks for {}...", file_hash_str);
        let mut file_metadata_path = self.files_path.clone();
        file_metadata_path.push(file_hash_str);

        // Try to read the file metadata. If it's corrupt, return an error signalling
        // that garbage collection needs to run.
        let chunk_hashes = match Self::read_metadata(&file_metadata_path).await {
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

        // Make sure the chunk hashes match with the file hash
        if !self.verify_file(file_hash, &chunk_hashes) {
            return Err(Error::GeodeNeedsGc);
        }

        let mut chunked_file = ChunkedFile::new(&chunk_hashes);

        // Open the file, if we can't we return the chunked file with no locally available chunk.
        let mut file = match File::open(&file_path).await {
            Ok(v) => v,
            Err(_) => {
                return Ok(chunked_file);
            }
        };

        // Iterate over chunks and find which chunks we have available locally.
        for (chunk_index, (chunk_hash, chunk_valid)) in chunked_file.0.iter_mut().enumerate() {
            let chunk = self.read_chunk(&mut file, &chunk_index).await?;

            // Perform chunk consistency check
            if !self.verify_chunk(chunk_hash, &chunk) {
                continue
            }

            *chunk_valid = Some(true);
        }

        Ok(chunked_file)
    }

    /// Fetch a single chunk from Geode. Returns a Vec containing the chunk content
    /// if it is found.
    pub async fn get_chunk(
        &self,
        chunk_hash: &blake3::Hash,
        file_hash: &blake3::Hash,
        file_path: &PathBuf,
    ) -> Result<Vec<u8>> {
        info!(target: "geode::get_chunk()", "[Geode] Getting chunk {}", hash_to_string(chunk_hash));

        if !file_path.exists() || !file_path.is_file() {
            return Err(Error::GeodeChunkNotFound)
        }

        let mut file_metadata_path = self.files_path.clone();
        file_metadata_path.push(hash_to_string(file_hash));

        // Try to read the file metadata. If it's corrupt, return an error signalling
        // that garbage collection needs to run.
        let chunk_hashes = match Self::read_metadata(&file_metadata_path).await {
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

        // Get the chunk index in the file from the chunk hash
        let chunk_index = match chunk_hashes.iter().position(|&h| h == *chunk_hash) {
            Some(index) => index,
            None => return Err(Error::GeodeChunkNotFound),
        };

        // Read the file to get the chunk content
        let mut file = File::open(&file_path).await?;
        let chunk = self.read_chunk(&mut file, &chunk_index).await?;

        // Perform chunk consistency check
        if !self.verify_chunk(chunk_hash, &chunk) {
            return Err(Error::GeodeNeedsGc)
        }

        Ok(chunk)
    }

    /// Read the file at `file_path` to get its chunk with index `chunk_index`.
    /// Returns the chunk content in a Vec.
    pub async fn read_chunk(
        &self,
        mut stream: impl AsyncRead + Unpin + AsyncSeek,
        chunk_index: &usize,
    ) -> Result<Vec<u8>> {
        let position = (*chunk_index as u64) * (MAX_CHUNK_SIZE as u64);
        let mut buf = [0u8; MAX_CHUNK_SIZE];
        stream.seek(SeekFrom::Start(position)).await?;
        let bytes_read = read_until_filled(stream, &mut buf).await?;
        Ok(buf[..bytes_read].to_vec())
    }

    /// Verifies that the file hash matches the chunk hashes.
    pub fn verify_file(&self, file_hash: &blake3::Hash, chunk_hashes: &[blake3::Hash]) -> bool {
        info!(target: "geode::verify_file()", "[Geode] Verifying file metadata for {}", hash_to_string(file_hash));

        let mut file_hasher = blake3::Hasher::new();
        for chunk_hash in chunk_hashes {
            file_hasher.update(chunk_hash.as_bytes());
        }

        *file_hash == file_hasher.finalize()
    }

    /// Verifies that the chunk hash matches the content.
    pub fn verify_chunk(&self, chunk_hash: &blake3::Hash, chunk_slice: &[u8]) -> bool {
        blake3::hash(chunk_slice) == *chunk_hash
    }
}
