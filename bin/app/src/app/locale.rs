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

use std::sync::mpsc::sync_channel;

#[cfg(target_os = "android")]
mod ui_consts {
    pub const LOCALE_PATH: &str = "lang/{locale}/{entry}";
}
#[cfg(all(
    any(target_os = "linux", target_os = "macos", target_os = "windows"),
    not(feature = "emulate-android")
))]
mod ui_consts {
    pub const LOCALE_PATH: &str = "assets/lang/{locale}/{entry}";
}

pub use ui_consts::*;

static ENTRIES: &[&'static str] = &[
    "app.ftl"
];

pub fn read_locale_ftl(locale: &str) -> String {
    let dir = LOCALE_PATH.replace("{locale}", locale);

    let mut output = String::new();
    for entry in ENTRIES {
        let path = dir.replace("{entry}", entry);
        let (sender, recvr) = sync_channel(1);
        miniquad::fs::load_file(&path, move |res| match res {
            Ok(res) => sender.send(res).unwrap(),
            Err(e) => panic!("FTL not found! {e}")
        });
        let res = recvr.recv().unwrap();
        let contents = std::str::from_utf8(&res).unwrap();
        output.push_str(contents);
    }
    output
}
