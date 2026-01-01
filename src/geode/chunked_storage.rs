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

use std::{
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};

use crate::geode::{file_sequence::FileSequence, MAX_CHUNK_SIZE};

#[derive(Clone, Debug, Eq)]
pub struct Chunk {
    pub hash: blake3::Hash,
    pub available: bool,
    pub size: usize,
}

impl Hash for Chunk {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hash.hash(state);
    }
}

impl PartialEq for Chunk {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash
    }
}

/// `ChunkedStorage` is a representation of a file or directory we're trying to
/// retrieve from `Geode`.
#[derive(Clone, Debug)]
pub struct ChunkedStorage {
    /// Vector of chunk hashes and a bool which is `true` if the chunk is
    /// available locally.
    chunks: Vec<Chunk>,
    /// FileSequence containing the list of file paths and file sizes, it has
    /// a single item if this is not a directory but a single file.
    fileseq: FileSequence,
    /// Set to `true` if this ChunkedStorage is the representation of a
    /// directory.
    is_dir: bool,
}

impl ChunkedStorage {
    pub fn new(hashes: &[blake3::Hash], files: &[(PathBuf, u64)], is_dir: bool) -> Self {
        Self {
            chunks: hashes
                .iter()
                .map(|x| Chunk { hash: *x, available: false, size: MAX_CHUNK_SIZE })
                .collect(),
            fileseq: FileSequence::new(files, is_dir),
            is_dir,
        }
    }

    /// Check whether we have all the chunks available locally.
    pub fn is_complete(&self) -> bool {
        !self.chunks.iter().any(|c| !c.available)
    }

    /// Return an iterator over the chunks and their availability.
    pub fn iter(&self) -> core::slice::Iter<'_, Chunk> {
        self.chunks.iter()
    }

    /// Return an mutable iterator over the chunks and their availability.
    pub fn iter_mut(&mut self) -> core::slice::IterMut<'_, Chunk> {
        self.chunks.iter_mut()
    }

    /// Return the number of chunks.
    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    /// Return `true` if the chunked file contains no chunk.
    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    /// Return the number of chunks available locally.
    pub fn local_chunks(&self) -> usize {
        self.chunks.iter().filter(|c| c.available).count()
    }

    /// Return `chunks`.
    pub fn get_chunks(&self) -> &Vec<Chunk> {
        &self.chunks
    }

    /// Return a mutable chunk from `chunks`.
    pub fn get_chunk_mut(&mut self, index: usize) -> &mut Chunk {
        &mut self.chunks[index]
    }

    pub fn get_chunk_index(&self, hash: &blake3::Hash) -> Option<usize> {
        for (i, chunk) in self.chunks.iter().enumerate() {
            if chunk.hash == *hash {
                return Some(i)
            }
        }
        None
    }

    /// Return the list of files from the `reader`.
    pub fn get_files(&self) -> &Vec<(PathBuf, u64)> {
        self.fileseq.get_files()
    }

    /// Return `fileseq`.
    pub fn get_fileseq(&self) -> &FileSequence {
        &self.fileseq
    }

    /// Return a mutable `fileseq`.
    pub fn get_fileseq_mut(&mut self) -> &mut FileSequence {
        &mut self.fileseq
    }

    /// Return `is_dir`.
    pub fn is_dir(&self) -> bool {
        self.is_dir
    }

    /// Return all chunks that contain parts of `file`.
    pub fn get_chunks_of_file(&self, file: &Path) -> Vec<blake3::Hash> {
        let files = self.fileseq.get_files();
        let file_index = files.iter().position(|(f, _)| f == file);
        if file_index.is_none() {
            return vec![];
        }
        let file_index = file_index.unwrap();

        let start_pos = self.fileseq.get_file_position(file_index);

        let end_pos = start_pos + files[file_index].1;

        let start_index = (start_pos as f64 / MAX_CHUNK_SIZE as f64).floor();
        let end_index = (end_pos as f64 / MAX_CHUNK_SIZE as f64).floor();

        let chunk_indexes: Vec<usize> = (start_index as usize..=end_index as usize).collect();

        chunk_indexes
            .iter()
            .filter_map(|&index| self.chunks.get(index))
            .map(|c| &c.hash)
            .cloned()
            .collect()
    }
}
