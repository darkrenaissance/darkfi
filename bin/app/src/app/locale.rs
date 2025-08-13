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

use std::{fs, path::Path};

#[cfg(target_os = "android")]
mod ui_consts {
    pub const LOCALE_PATH: &str = "lang/{locale}/";
}
#[cfg(all(
    any(target_os = "linux", target_os = "macos", target_os = "windows"),
    not(feature = "emulate-android")
))]
mod ui_consts {
    pub const LOCALE_PATH: &str = "assets/lang/{locale}/";
}

pub use ui_consts::*;

fn read_files_to_string<P: AsRef<Path>>(dir: P) -> std::io::Result<String> {
    let mut output = String::new();

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            let contents = fs::read_to_string(&path)?;
            output.push_str(&contents);
        }
    }

    Ok(output)
}

pub fn read_locale_ftl(locale: &str) -> String {
    let dir = LOCALE_PATH.replace("{locale}", locale);
    read_files_to_string(dir).unwrap()
}
