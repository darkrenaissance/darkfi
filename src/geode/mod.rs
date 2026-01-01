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

//! Chunk-based file storage implementation.
//! This is a building block for a DHT or something similar.
//!
//! The API supports file/directory insertion and retrieval. There is
//! intentionally no `remove` support. File removal should be handled
//! externally, and then it is only required to run `garbage_collect()` to
//! clean things up.
//!
//! The hash of a file is the BLAKE3 hash of hashed chunks in the correct
//! order.
//! The hash of a directory is the BLAKE3 hash of hashed chunks in the correct
//! order and the ordered list of (file path, file sizes).
//! All hashes (file, directory, chunk) are 32 bytes long, and are encoded in
//! base58 whenever necessary.
//!
//! The filesystem hierarchy stores a `files` directory storing metadata
//! about a full file and a `directories` directory storing metadata about all
//! files in a directory (all subdirectories included).
//! The filename of a file in `files` or `directories` is the hash of the
//! file/directory as defined above.
//! Inside a file in `files` is the ordered list of the chunks making up the
//! full file.
//! Inside a file in `directories` is the ordered list of the chunks making up
//! each full file, and the (relative) file path and hash of all files in the
//! directory.
//!
//! To get the chunks you split the full file into `MAX_CHUNK_SIZE` sized
//! slices, the last chunk is the only one that can be smaller than that.
//!
//! It might look like the following:
//! ```
//! /files/B9fFKaEYphw2oH5PDbeL1TTAcSzL6ax84p8SjBKzuYzX
//! /files/8nA3ndjFFee3n5wMPLZampLpGaMJi3od4MSyaXPDoF91
//! /files/...
//! /directories/FXDduPcEohVzsSxtNVSFU64qtYxEVEHBMkF4k5cBvt3B
//! /directories/AHjU1LizfGqsGnF8VSa9kphSQ5pqS4YjmPqme5RZajsj
//! /directories/...
//! ```
//!
//! Inside a file metadata (file in `files`) is the ordered list of chunk
//! hashes, for example:
//! ```
//! 2bQPxSR8Frz7S7JW3DRAzEtkrHfLXB1CN65V7az77pUp
//! CvjvN6MfWQYK54DgKNR7MPgFSZqsCgpWKF2p8ot66CCP
//! ```
//!
//! Inside a directory metadata (file in `directories`) is, in addition to
//! chunk hashes, the path and size of each file in the directory. For example:
//! ```
//! 8Kb55jeqJsq7WTBN93gvBzh2zmXAXVPh111VqD3Hi42V
//! GLiBqpLPTbpJhSMYfzi3s7WivrTViov7ShX7uso6fG5s
//! picture.jpg 312948
//! ```
//! Chunks of a directory can include multiple files, if multiple files fit
//! into `MAX_CHUNK_SIZE`. The chunks are computed as if all the files were
//! concatenated into a single big file, to minimize the number of chunks.
//!
//! The full file is not copied, and individual chunks are not stored by
//! geode. Additionally it does not keep track of the full files path.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use futures::{AsyncRead, AsyncSeek};
use smol::{
    fs::{self, File},
    io::{
        AsyncBufReadExt, AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufReader, Cursor, ErrorKind,
        SeekFrom,
    },
    stream::StreamExt,
};
use tracing::{debug, info, warn};

use crate::{Error, Result};

mod chunked_storage;
pub use chunked_storage::{Chunk, ChunkedStorage};

mod file_sequence;
pub use file_sequence::FileSequence;

mod util;
pub use util::{hash_to_string, read_until_filled};

/// Defined maximum size of a stored chunk (256 KiB)
pub const MAX_CHUNK_SIZE: usize = 262_144;

/// Path prefix where file metadata is stored
const FILES_PATH: &str = "files";

/// Path prefix where directory metadata is stored
const DIRS_PATH: &str = "directories";

/// Chunk-based file storage interface.
pub struct Geode {
    /// Path to the filesystem directory where file metadata is stored
    pub files_path: PathBuf,
    /// Path to the filesystem directory where directory metadata is stored
    pub dirs_path: PathBuf,
}

impl Geode {
    /// Instantiate a new [`Geode`] object.
    /// `base_path` defines the root directory where Geode will store its
    /// file metadata and chunks.
    pub async fn new(base_path: &PathBuf) -> Result<Self> {
        let mut files_path: PathBuf = base_path.into();
        files_path.push(FILES_PATH);
        let mut dirs_path: PathBuf = base_path.into();
        dirs_path.push(DIRS_PATH);

        // Create necessary directory structure if needed
        fs::create_dir_all(&files_path).await?;
        fs::create_dir_all(&dirs_path).await?;

        Ok(Self { files_path, dirs_path })
    }

    /// Attempt to read chunk hashes and files metadata from a given metadata path.
    /// This works for both file metadata and directory metadata.
    /// Returns (chunk hashes, [(file path, file size)]).
    async fn read_metadata(path: &PathBuf) -> Result<(Vec<blake3::Hash>, Vec<(PathBuf, u64)>)> {
        debug!(target: "geode::read_dir_metadata", "Reading chunks from {path:?} (dir)");

        let mut chunk_hashes = vec![];
        let mut files = vec![];

        let fd = File::open(path).await?;
        let mut lines = BufReader::new(fd).lines();

        while let Some(line) = lines.next().await {
            let line = line?;
            let line = line.trim();

            if line.is_empty() {
                continue; // Skip empty lines
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() == 2 {
                // File
                let file_path = PathBuf::from(parts[0]);
                if file_path.clone().is_absolute() {
                    return Err(Error::Custom(format!(
                        "Path of file {} is absolute, which is not allowed",
                        parts[0]
                    )))
                }

                // Check for `..` in the path components
                for component in file_path.clone().components() {
                    if component == std::path::Component::ParentDir {
                        return Err(Error::Custom(format!("Path of file {} contains reference to parent dir, which is not allowed", parts[0])))
                    }
                }

                let file_size = parts[1].parse::<u64>()?;
                files.push((file_path, file_size));
            } else if parts.len() == 1 {
                // Chunk
                let chunk_hash_str = parts[0].trim();
                if chunk_hash_str.is_empty() {
                    break; // Stop reading chunk hashes on empty line
                }
                let mut hash_buf = [0u8; 32];
                bs58::decode(chunk_hash_str).onto(&mut hash_buf)?;
                let chunk_hash = blake3::Hash::from_bytes(hash_buf);
                chunk_hashes.push(chunk_hash);
            } else {
                // Invalid format
                return Err(Error::Custom("Invalid directory metadata format".to_string()));
            }
        }

        Ok((chunk_hashes, files))
    }

    /// Perform garbage collection over the filesystem hierarchy.
    /// Returns a set representing deleted files.
    pub async fn garbage_collect(&self) -> Result<HashSet<blake3::Hash>> {
        info!(target: "geode::garbage_collect", "[Geode] Performing garbage collection");
        // We track corrupt files here.
        let mut deleted_files = HashSet::new();

        // Perform health check over metadata. For now we just ensure they
        // have the correct format.
        let file_paths = fs::read_dir(&self.files_path).await?;
        let dir_paths = fs::read_dir(&self.dirs_path).await?;
        let mut paths = file_paths.chain(dir_paths);
        while let Some(file) = paths.next().await {
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
            let hash = match bs58::decode(file_name).onto(&mut hash_buf) {
                Ok(_) => blake3::Hash::from_bytes(hash_buf),
                Err(_) => continue,
            };

            // The filename is a BLAKE3 hash. It should contain a newline-separated
            // list of chunks which represent the full file. If that is not the case
            // we will consider it a corrupted file and delete it.
            if Self::read_metadata(&path).await.is_err() {
                if let Err(e) = fs::remove_file(path).await {
                    warn!(
                       target: "geode::garbage_collect",
                       "[Geode] Garbage collect failed to remove corrupted metadata: {e}"
                    );
                }

                deleted_files.insert(hash);
                continue
            }
        }

        info!(target: "geode::garbage_collect", "[Geode] Garbage collection finished");
        Ok(deleted_files)
    }

    /// Chunk a stream.
    /// Returns a hasher (containing the chunk hashes), and the list of chunk hashes.
    pub async fn chunk_stream(
        &self,
        mut stream: impl AsyncRead + Unpin,
    ) -> Result<(blake3::Hasher, Vec<blake3::Hash>)> {
        let mut hasher = blake3::Hasher::new();
        let mut chunk_hashes = vec![];

        loop {
            let mut buf = vec![0u8; MAX_CHUNK_SIZE];
            let bytes_read = stream.read(&mut buf).await?;
            if bytes_read == 0 {
                break
            }

            let chunk_hash = blake3::hash(&buf[..bytes_read]);
            hasher.update(chunk_hash.as_bytes());
            chunk_hashes.push(chunk_hash);
        }

        Ok((hasher, chunk_hashes))
    }

    /// Sorts files by their PathBuf.
    pub fn sort_files(&self, files: &mut [(PathBuf, u64)]) {
        files.sort_by(|(a, _), (b, _)| a.to_string_lossy().cmp(&b.to_string_lossy()));
    }

    /// Add chunk hashes to `hasher`.
    pub fn hash_chunks_metadata(&self, hasher: &mut blake3::Hasher, chunk_hashes: &[blake3::Hash]) {
        for chunk in chunk_hashes {
            hasher.update(chunk.as_bytes());
        }
    }

    /// Add files metadata to `hasher`.
    /// You must sort the files using `sort_files`.
    pub fn hash_files_metadata(
        &self,
        hasher: &mut blake3::Hasher,
        relative_files: &[(PathBuf, u64)],
    ) {
        for file in relative_files {
            hasher.update(file.0.to_string_lossy().to_string().as_bytes());
            hasher.update(&file.1.to_le_bytes());
        }
    }

    /// Create and insert file or directory metadata into Geode.
    /// Always overwrites any existing file.
    /// Verifies that the metadata is valid.
    /// The `relative_files` slice is empty for files.
    pub async fn insert_metadata(
        &self,
        hash: &blake3::Hash,
        chunk_hashes: &[blake3::Hash],
        relative_files: &[(PathBuf, u64)],
    ) -> Result<()> {
        info!(target: "geode::insert_metadata", "[Geode] Inserting metadata");

        // Verify the metadata
        if !self.verify_metadata(hash, chunk_hashes, relative_files) {
            return Err(Error::GeodeNeedsGc)
        }

        // Write the metadata file
        let mut file_path = match relative_files.is_empty() {
            true => self.files_path.clone(),
            false => self.dirs_path.clone(),
        };
        file_path.push(hash_to_string(hash).as_str());
        let mut file_fd = File::create(&file_path).await?;

        for ch in chunk_hashes {
            file_fd.write(format!("{}\n", hash_to_string(ch).as_str()).as_bytes()).await?;
        }
        for file in relative_files {
            file_fd.write(format!("{} {}\n", file.0.to_string_lossy(), file.1).as_bytes()).await?;
        }
        file_fd.flush().await?;

        Ok(())
    }

    /// Write a single chunk given a stream.
    /// The file must be inserted into Geode before calling this method.
    /// Always overwrites any existing chunk. Returns the chunk hash and
    /// the number of bytes written to the file system.
    pub async fn write_chunk(
        &self,
        chunked: &mut ChunkedStorage,
        stream: impl AsRef<[u8]>,
    ) -> Result<(blake3::Hash, usize)> {
        info!(target: "geode::write_chunk", "[Geode] Writing single chunk");

        let mut cursor = Cursor::new(&stream);
        let mut chunk = vec![0u8; MAX_CHUNK_SIZE];

        // Read the stream to get the chunk content
        let chunk_slice = read_until_filled(&mut cursor, &mut chunk).await?;

        // Get the chunk hash from the content
        let chunk_hash = blake3::hash(chunk_slice);

        // Get the chunk index in the file/directory from the chunk hash
        let chunk_index = match chunked.iter().position(|c| c.hash == chunk_hash) {
            Some(index) => index,
            None => {
                return Err(Error::GeodeNeedsGc);
            }
        };

        // Compute byte position from the chunk index and the chunk size
        let position = (chunk_index as u64) * (MAX_CHUNK_SIZE as u64);

        // Seek to the correct position
        let fileseq = &mut chunked.get_fileseq_mut();
        fileseq.seek(SeekFrom::Start(position)).await?;

        // This will write the chunk, and truncate files if `chunked` is a directory.
        let bytes_written = fileseq.write(chunk_slice).await?;

        // If it's the last chunk of a file (and it's *not* a directory),
        // truncate the file to the correct length.
        // This is because contrary to directories, we do not know the exact
        // file size from its metadata, we only know the number of chunks.
        // Therefore we only know the exact size once we know the size of the
        // last chunk.
        // We also update the `FileSequence` to the exact size.
        if !chunked.is_dir() && chunk_index == chunked.len() - 1 {
            let exact_file_size =
                chunked.len() * MAX_CHUNK_SIZE - (MAX_CHUNK_SIZE - chunk_slice.len());
            if let Some(file) = &chunked.get_fileseq_mut().get_current_file() {
                let _ = file.set_len(exact_file_size as u64);
            }
            chunked.get_fileseq_mut().set_file_size(0, exact_file_size as u64);
        }

        Ok((chunk_hash, bytes_written))
    }

    /// Fetch file/directory metadata from Geode. Returns [`ChunkedStorage`]. Returns an error if
    /// the read failed in any way (could also be the file does not exist).
    pub async fn get(&self, hash: &blake3::Hash, path: &Path) -> Result<ChunkedStorage> {
        let hash_str = hash_to_string(hash);
        info!(target: "geode::get", "[Geode] Getting chunks for {hash_str}...");

        // Try to read the file or dir metadata. If it's corrupt, return an error signalling
        // that garbage collection needs to run.
        let metadata_paths = [self.files_path.join(&hash_str), self.dirs_path.join(&hash_str)];
        for metadata_path in metadata_paths {
            match Self::read_metadata(&metadata_path).await {
                Ok((chunk_hashes, files)) => {
                    return self.create_chunked_storage(hash, path, &chunk_hashes, &files).await
                }
                Err(e) => {
                    if !matches!(e, Error::Io(ErrorKind::NotFound)) {
                        return Err(Error::GeodeNeedsGc)
                    }
                }
            };
        }

        Err(Error::GeodeFileNotFound)
    }

    /// Create a ChunkedStorage from metadata.
    /// `hash` is the hash of the file or directory.
    async fn create_chunked_storage(
        &self,
        hash: &blake3::Hash,
        path: &Path,
        chunk_hashes: &[blake3::Hash],
        relative_files: &[(PathBuf, u64)], // Only used by directories
    ) -> Result<ChunkedStorage> {
        // Make sure the file or directory is valid
        if !self.verify_metadata(hash, chunk_hashes, relative_files) {
            return Err(Error::GeodeNeedsGc);
        }

        let chunked = if relative_files.is_empty() {
            // File
            let file_size = (chunk_hashes.len() * MAX_CHUNK_SIZE) as u64; // Upper bound, not actual file size
            ChunkedStorage::new(chunk_hashes, &[(path.to_path_buf(), file_size)], false)
        } else {
            // Directory
            let files: Vec<_> = relative_files
                .iter()
                .map(|(file_path, size)| (path.join(file_path), *size))
                .collect();
            ChunkedStorage::new(chunk_hashes, &files, true)
        };

        Ok(chunked)
    }

    /// Fetch a single chunk from Geode. Returns a Vec containing the chunk content
    /// if it is found.
    /// The returned chunk is NOT verified.
    pub async fn get_chunk(
        &self,
        chunked: &mut ChunkedStorage,
        chunk_hash: &blake3::Hash,
    ) -> Result<Vec<u8>> {
        info!(target: "geode::get_chunk", "[Geode] Getting chunk {}", hash_to_string(chunk_hash));

        // Get the chunk index in the file from the chunk hash
        let chunk_index = match chunked.iter().position(|c| c.hash == *chunk_hash) {
            Some(index) => index,
            None => return Err(Error::GeodeChunkNotFound),
        };

        // Read the file to get the chunk content
        let chunk = self.read_chunk(&mut chunked.get_fileseq_mut(), &chunk_index).await?;

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
        let mut buf = vec![0u8; MAX_CHUNK_SIZE];
        stream.seek(SeekFrom::Start(position)).await?;
        let bytes_read = stream.read(&mut buf).await?;
        Ok(buf[..bytes_read].to_vec())
    }

    /// Verifies that the file hash matches the chunk hashes.
    pub fn verify_metadata(
        &self,
        hash: &blake3::Hash,
        chunk_hashes: &[blake3::Hash],
        files: &[(PathBuf, u64)],
    ) -> bool {
        info!(target: "geode::verify_metadata", "[Geode] Verifying metadata for {}", hash_to_string(hash));
        let mut hasher = blake3::Hasher::new();
        self.hash_chunks_metadata(&mut hasher, chunk_hashes);
        self.hash_files_metadata(&mut hasher, files);
        *hash == hasher.finalize()
    }

    /// Verifies that the chunk hash matches the content.
    pub fn verify_chunk(&self, chunk_hash: &blake3::Hash, chunk_slice: &[u8]) -> bool {
        info!(target: "geode::verify_chunk", "[Geode] Verifying chunk {}", hash_to_string(chunk_hash));
        blake3::hash(chunk_slice) == *chunk_hash
    }
}
