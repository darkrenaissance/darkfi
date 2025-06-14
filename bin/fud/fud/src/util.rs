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

use darkfi::Result;
use smol::{fs, stream::StreamExt};
use std::path::{Path, PathBuf};

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
