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

use smol::{
    fs::{self, File},
    stream::StreamExt,
};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

pub use darkfi::geode::hash_to_string;
use darkfi::{
    net::{Message, MessageSubscription},
    Error, Result,
};

use crate::proto::ResourceMessage;

pub async fn get_all_files(dir: &Path) -> Result<Vec<(PathBuf, u64)>> {
    let mut files = Vec::new();

    let mut entries = fs::read_dir(dir).await.unwrap();

    while let Some(entry) = entries.try_next().await.unwrap() {
        let path = entry.path();

        if path.is_dir() {
            files.append(&mut Box::pin(get_all_files(&path)).await?);
        } else {
            let metadata = fs::metadata(&path).await?;
            let file_size = metadata.len();
            files.push((path, file_size));
        }
    }

    Ok(files)
}

pub async fn create_all_files(files: &[PathBuf]) -> Result<()> {
    for file_path in files.iter() {
        if !file_path.exists() {
            if let Some(dir) = file_path.parent() {
                fs::create_dir_all(dir).await?;
            }
            File::create(&file_path).await?;
        }
    }

    Ok(())
}

/// An enum to represent a set of files, where you can use `All` if you want
/// all files without having to specify all of them.
#[derive(Clone, Debug)]
pub enum FileSelection {
    All,
    Set(HashSet<PathBuf>),
}

impl FileSelection {
    pub fn get_set(&self) -> Option<HashSet<PathBuf>> {
        match self {
            FileSelection::Set(set) => Some(set.clone()),
            _ => None,
        }
    }

    pub fn is_all(&self) -> bool {
        matches!(self, FileSelection::All)
    }

    pub fn is_subset(&self, other: &Self) -> bool {
        match self {
            FileSelection::All => other.is_all(),
            FileSelection::Set(self_set) => match other {
                FileSelection::All => true,
                FileSelection::Set(other_set) => self_set.is_subset(other_set),
            },
        }
    }

    pub fn is_disjoint(&self, other: &Self) -> bool {
        match self {
            FileSelection::All => false,
            FileSelection::Set(self_set) => match other {
                FileSelection::All => false,
                FileSelection::Set(other_set) => self_set.is_disjoint(other_set),
            },
        }
    }

    /// Merges two file selections: if any is [`FileSelection::All`] then
    /// output is [`FileSelection::All`], if they are both
    /// [`FileSelection::Set`] then the sets are merged.
    pub fn merge(&self, other: &Self) -> Self {
        if matches!(self, FileSelection::All) || matches!(other, FileSelection::All) {
            return FileSelection::All
        }
        let files1 = self.get_set().unwrap();
        let files2 = other.get_set().unwrap();
        FileSelection::Set(files1.union(&files2).cloned().collect())
    }
}

impl FromIterator<PathBuf> for FileSelection {
    fn from_iter<I: IntoIterator<Item = PathBuf>>(iter: I) -> Self {
        let paths: HashSet<PathBuf> = iter.into_iter().collect();
        FileSelection::Set(paths)
    }
}

/// Wait for a [`crate::proto::ResourceMessage`] on `msg_subscriber` with a timeout.
/// If we receive a message with the wrong resource hash, it's skipped.
pub async fn receive_resource_msg<M: Message + ResourceMessage + std::fmt::Debug>(
    msg_subscriber: &MessageSubscription<M>,
    resource_hash: blake3::Hash,
    timeout_seconds: u64,
) -> Result<Arc<M>> {
    let start = Instant::now();
    loop {
        let elapsed = start.elapsed().as_secs();
        if elapsed >= timeout_seconds {
            return Err(Error::ConnectTimeout);
        }
        let remaining_timeout = timeout_seconds - elapsed;

        let reply = msg_subscriber.receive_with_timeout(remaining_timeout).await?;
        // Done if it's the right resource hash
        if reply.resource_hash() == resource_hash {
            return Ok(reply)
        }
    }
}
