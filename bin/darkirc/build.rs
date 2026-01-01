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

use std::process::Command;

fn main() {
    let output = Command::new("git").args(["rev-parse", "--short", "HEAD"]).output();

    if let Ok(output) = output {
        if output.status.success() {
            let commitish = String::from_utf8_lossy(&output.stdout);
            println!("cargo:rustc-env=COMMITISH={}", commitish.trim());
        }
    }

    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "android" {
        println!("cargo:rustc-link-search={}/sqlcipher", env!("CARGO_MANIFEST_DIR"));
    }
}
