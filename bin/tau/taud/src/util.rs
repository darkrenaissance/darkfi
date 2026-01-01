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
    fs::{File, OpenOptions},
    os::unix::prelude::OpenOptionsExt,
    path::Path,
};

use tracing::debug;

use darkfi::{Error, Result};
use rand::{distributions::Alphanumeric, rngs::OsRng, Rng};

use crate::task_info::{TaskEvent, TaskInfo};

pub fn set_event(task_info: &mut TaskInfo, action: &str, author: &str, content: &str) {
    debug!(target: "tau", "TaskInfo::set_event()");
    if !content.is_empty() {
        task_info.events.push(TaskEvent::new(action.into(), author.into(), content.into()));
    }
}

pub fn pipe_write<P: AsRef<Path>>(path: P) -> Result<File> {
    OpenOptions::new().append(true).custom_flags(libc::O_NONBLOCK).open(path).map_err(Error::from)
}

pub fn gen_id(len: usize) -> String {
    OsRng.sample_iter(&Alphanumeric).take(len).map(char::from).collect()
}
