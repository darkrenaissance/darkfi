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
    fs::{File, OpenOptions},
    os::unix::prelude::OpenOptionsExt,
    path::Path,
};

use log::debug;

use darkfi::{Error, Result};

use crate::task_info::{TaskEvent, TaskInfo};
/*
pub fn find_free_id(task_ids: &[u32]) -> u32 {
    for i in 1.. {
        if !task_ids.contains(&i) {
            return i
        }
    }
    1
}
 */

pub fn set_event(task_info: &mut TaskInfo, action: &str, author: &str, content: &str) {
    debug!(target: "tau", "TaskInfo::set_event()");
    if !content.is_empty() {
        task_info.events.push(TaskEvent::new(action.into(), author.into(), content.into()));
    }
}

pub fn pipe_write<P: AsRef<Path>>(path: P) -> Result<File> {
    OpenOptions::new()
        .write(true)
        .append(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .map_err(Error::from)
}

// #[cfg(test)]
// mod tests {
//     use super::*;

//     use darkfi::Result;
//     #[test]
//     fn find_free_id_test() -> Result<()> {
//         let mut ids: Vec<u32> = vec![1, 3, 8, 9, 10, 3];
//         let ids_empty: Vec<u32> = vec![];
//         let ids_duplicate: Vec<u32> = vec![1; 100];

//         let find_id = find_free_id(&ids);

//         assert_eq!(find_id, 2);

//         ids.push(find_id);

//         assert_eq!(find_free_id(&ids), 4);

//         assert_eq!(find_free_id(&ids_empty), 1);

//         assert_eq!(find_free_id(&ids_duplicate), 2);

//         Ok(())
//     }
// }
