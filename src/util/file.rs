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

use std::{
    fs::File,
    io::{BufReader, Read, Write},
    path::Path,
};

use serde::{de::DeserializeOwned, Serialize};

use crate::Result;

pub fn load_file(path: &Path) -> Result<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut st = String::new();
    reader.read_to_string(&mut st)?;
    Ok(st)
}

pub fn save_file(path: &Path, st: &str) -> Result<()> {
    let mut file = File::create(path)?;
    file.write_all(st.as_bytes())?;
    Ok(())
}

pub fn load_json_file<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    let value: T = serde_json::from_reader(reader)?;
    Ok(value)
}

pub fn save_json_file<T: Serialize>(path: &Path, value: &T, pretty: bool) -> Result<()> {
    let file = File::create(path)?;

    if pretty {
        serde_json::to_writer_pretty(file, value)?;
    } else {
        serde_json::to_writer(file, value)?;
    }

    Ok(())
}
