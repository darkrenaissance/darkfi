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

use futures::{
    task::{Context, Poll},
    AsyncRead, AsyncSeek, AsyncWrite,
};
use smol::{
    fs::{File, OpenOptions},
    io::{self, AsyncReadExt, AsyncSeekExt, AsyncWriteExt, SeekFrom},
};
use std::{collections::HashSet, path::PathBuf, pin::Pin};

/// `FileSequence` is an object that implements `AsyncRead`, `AsyncSeek`, and
/// `AsyncWrite` for an ordered list of (file path, file size).
///
/// You can use it to read and write from/to a list a file, without having to
/// manage individual file operations explicitly.
///
/// This allows seamless handling of multiple files as if they were a single
/// continuous file. It automatically opens the next file in the list when the
/// current file is exhausted.
///
/// It's also made so that files in `files` that do not exist on the filesystem
/// will get skipped, without returning an Error. All files you want to read,
/// write, and seek to should be created before using the FileSequence.
#[derive(Debug)]
pub struct FileSequence {
    /// List of (file path, file size). File sizes are not the sizes of the
    /// files as they currently are on the file system, but the sizes we want
    files: Vec<(PathBuf, u64)>,
    /// Currently opened file
    current_file: Option<File>,
    /// Index of the currently opened file in the `files` vector
    current_file_index: Option<usize>,

    position: u64,
    /// Set to `true` to automatically set the length of the file on the
    /// filesystem to it's size as defined in the `files` vector, after a write
    auto_set_len: bool,
}

impl FileSequence {
    pub fn new(files: &[(PathBuf, u64)], auto_set_len: bool) -> Self {
        Self {
            files: files.to_vec(),
            current_file: None,
            current_file_index: None,
            position: 0,
            auto_set_len,
        }
    }

    /// Update a single file size.
    pub fn set_file_size(&mut self, file_index: usize, file_size: u64) {
        self.files[file_index].1 = file_size;
    }

    /// Return `current_file`.
    pub fn get_current_file(&self) -> &Option<File> {
        &self.current_file
    }

    /// Return `files`.
    pub fn get_files(&self) -> &Vec<(PathBuf, u64)> {
        &self.files
    }

    /// Return the combined file size of all files.
    pub fn len(&self) -> u64 {
        self.files.iter().map(|(_, size)| size).sum()
    }

    /// Return `true` if the `FileSequence` contains no file.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Return the combined file size of all files.
    pub fn subset_len(&self, files: HashSet<PathBuf>) -> u64 {
        self.files.iter().filter(|(path, _)| files.contains(path)).map(|(_, size)| size).sum()
    }

    /// Compute the starting position of the file (in bytes) by suming up
    /// the size of the previous files.
    pub fn get_file_position(&self, file_index: usize) -> u64 {
        let mut pos = 0;
        for i in 0..file_index {
            pos += self.files[i].1;
        }
        pos
    }

    /// Open the file at (`current_file_index` + 1).
    /// If no file is currently open (`current_file_index` is None), it opens
    /// the first file.
    async fn open_next_file(&mut self) -> io::Result<()> {
        self.current_file = None;
        self.current_file_index = match self.current_file_index {
            Some(i) => Some(i + 1),
            None => Some(0),
        };
        if self.current_file_index.unwrap() >= self.files.len() {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "No more files to open"))
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(false)
            .open(self.files[self.current_file_index.unwrap()].0.clone())
            .await?;
        self.current_file = Some(file);
        Ok(())
    }

    /// Open the file at `file_index`.
    async fn open_file(&mut self, file_index: usize) -> io::Result<()> {
        self.current_file = None;
        self.current_file_index = Some(file_index);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(false)
            .open(self.files[file_index].0.clone())
            .await?;
        self.current_file = Some(file);
        Ok(())
    }
}

impl AsyncRead for FileSequence {
    fn poll_read(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        let mut total_read = 0;

        while total_read < buf.len() {
            if this.current_file.is_none() {
                if let Some(file_index) = this.current_file_index {
                    // Stop if there are no more files to read
                    if file_index >= this.files.len() - 1 {
                        return Poll::Ready(Ok(total_read));
                    }
                    let start_pos = this.get_file_position(file_index);
                    let file_size = this.files[file_index].1 as usize;
                    let file_pos = this.position - start_pos;
                    let space_left = file_size - file_pos as usize;
                    let skip_bytes = (buf.len() - total_read).min(space_left);
                    this.position += skip_bytes as u64;
                    total_read += skip_bytes;
                }

                // Open the next file
                match smol::block_on(this.open_next_file()) {
                    Ok(_) => {}
                    Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                        return Poll::Ready(Ok(total_read));
                    }
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {
                        this.current_file = None;
                        continue; // Skip to next file
                    }
                    Err(e) => return Poll::Ready(Err(e)),
                }
            }

            // Read from the current file
            let file = this.current_file.as_mut().unwrap();
            match smol::block_on(file.read(&mut buf[total_read..])) {
                Ok(bytes_read) => {
                    if bytes_read == 0 {
                        this.current_file = None; // Move to the next file
                    } else {
                        total_read += bytes_read;
                        this.position += bytes_read as u64;
                    }
                }
                Err(e) => return Poll::Ready(Err(e)),
            }
        }

        Poll::Ready(Ok(total_read))
    }
}

impl AsyncSeek for FileSequence {
    fn poll_seek(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        pos: SeekFrom,
    ) -> Poll<io::Result<u64>> {
        let this = self.get_mut();

        let abs_pos = match pos {
            SeekFrom::Start(offset) => offset,
            _ => todo!(), // TODO
        };

        // Determine which file to seek in
        let mut file_index = 0;
        let mut bytes_offset = 0;

        while file_index < this.files.len() {
            if bytes_offset + this.files[file_index].1 >= abs_pos {
                break;
            }
            bytes_offset += this.files[file_index].1;
            file_index += 1;
        }

        if file_index >= this.files.len() {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Seek position out of bounds",
            )))
        }

        this.position = abs_pos; // Update FileSequence position

        // Open the file
        if this.current_file.is_none() ||
            this.current_file_index.is_some() && this.current_file_index.unwrap() != file_index
        {
            match smol::block_on(this.open_file(file_index)) {
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    // If the file does not exist, return without actually seeking it
                    return Poll::Ready(Ok(this.position));
                }
                Err(e) => return Poll::Ready(Err(e)),
            };
        }

        let file = this.current_file.as_mut().unwrap();
        let file_pos = abs_pos - bytes_offset;

        // Seek in the current file
        match smol::block_on(file.seek(SeekFrom::Start(file_pos))) {
            Ok(_) => Poll::Ready(Ok(this.position)),
            Err(e) => Poll::Ready(Err(e)),
        }
    }
}

impl AsyncWrite for FileSequence {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        let mut total_bytes_written = 0;
        let mut remaining_buf = buf;
        let auto_set_len = this.auto_set_len;

        let finalize_current_file = |file: &mut File, max_size: u64| {
            if auto_set_len {
                smol::block_on(file.set_len(max_size))?;
            }
            smol::block_on(file.flush())?;
            Ok(())
        };

        loop {
            // Ensure the current file is open
            if this.current_file.is_none() {
                if let Some(file_index) = this.current_file_index {
                    if file_index >= this.files.len() - 1 {
                        break; // No more files
                    }
                    if remaining_buf.is_empty() {
                        break; // No more data to write
                    }
                    let start_pos = this.get_file_position(file_index);
                    let file_size = this.files[file_index].1 as usize;
                    let file_pos = this.position - start_pos;
                    let space_left = file_size - file_pos as usize;
                    let skip_bytes = remaining_buf.len().min(space_left);
                    this.position += skip_bytes as u64;
                    remaining_buf = &remaining_buf[skip_bytes..]; // Update the remaining buffer
                }

                // Switch to the next file
                match smol::block_on(this.open_next_file()) {
                    Ok(_) => {}
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {
                        this.current_file = None;
                        continue; // Skip to next file
                    }
                    Err(e) => return Poll::Ready(Err(e)),
                }
            }

            let file = this.current_file.as_mut().unwrap();
            let max_size = this.files[this.current_file_index.unwrap()].1;

            // Check how much space is left in the current file
            let current_position = smol::block_on(file.seek(io::SeekFrom::Current(0)))?;
            let space_left = max_size - current_position;
            let bytes_to_write = remaining_buf.len().min(space_left as usize);

            if bytes_to_write == 0 {
                // Continue to the next iteration to check the new file
                if let Err(e) = finalize_current_file(file, max_size) {
                    return Poll::Ready(Err(e));
                }
                this.current_file = None;
                continue;
            }

            // Write to the current file
            match smol::block_on(file.write(&remaining_buf[..bytes_to_write])) {
                Ok(bytes_written) => {
                    total_bytes_written += bytes_written;
                    this.position += bytes_written as u64;
                    remaining_buf = &remaining_buf[bytes_written..]; // Update the remaining buffer
                    if remaining_buf.is_empty() {
                        if let Err(e) = finalize_current_file(file, max_size) {
                            return Poll::Ready(Err(e));
                        }
                        break; // No more data to write
                    }

                    // We wrote to the end of this file, use new file on next iteration
                    if bytes_written == bytes_to_write {
                        if let Err(e) = finalize_current_file(file, max_size) {
                            return Poll::Ready(Err(e));
                        }
                        this.current_file = None;
                    }
                }
                Err(e) => return Poll::Ready(Err(e)), // Return error if write fails
            }
        }

        Poll::Ready(Ok(total_bytes_written))
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(())) // TODO
    }

    fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if let Some(file) = this.current_file.take() {
            match smol::block_on(file.sync_all()) {
                Ok(()) => Poll::Ready(Ok(())),
                Err(e) => Poll::Ready(Err(e)),
            }
        } else {
            Poll::Ready(Ok(())) // No file to close
        }
    }
}
