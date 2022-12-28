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

use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct RaftSettings {
    // the leader duration for sending heartbeat; in milliseconds
    pub heartbeat_timeout: u64,

    // the duration for electing new leader; in seconds
    pub timeout: u64,

    // the duration for sending id to other nodes; in seconds
    pub id_timeout: u64,

    // this duration used to clean up hashmaps; in seconds
    pub prun_duration: i64,

    // Datastore path
    pub datastore_path: PathBuf,
}

impl Default for RaftSettings {
    fn default() -> Self {
        Self {
            heartbeat_timeout: 500,
            timeout: 6,
            id_timeout: 12,
            prun_duration: 30,
            datastore_path: PathBuf::from(""),
        }
    }
}
