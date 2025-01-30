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
    fs::File,
    io::{BufReader, Read, Write},
    path::Path,
};

use tinyjson::JsonValue;

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

pub fn load_json_file(path: &Path) -> Result<JsonValue> {
    let st = load_file(path)?;
    Ok(st.parse()?)
}

pub fn save_json_file(path: &Path, value: &JsonValue, pretty: bool) -> Result<()> {
    let mut file = File::create(path)?;

    if pretty {
        value.format_to(&mut file)?;
    } else {
        value.write_to(&mut file)?;
    }

    Ok(())
}
