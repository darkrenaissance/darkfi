/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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
    env,
    fs::{self, File},
    io::Write,
    process::Command,
};

use chrono::{Datelike, Local, NaiveDate};
use log::error;

use darkfi::{util::time::Timestamp, Result};

use crate::{
    primitives::TaskInfo,
    view::{comments_table, events_table, taskinfo_table},
};

/// Parse due date (e.g. "1503" for 15 March) as i64 timestamp.
pub fn due_as_timestamp(due: &str) -> Option<u64> {
    if due.len() != 4 || due.parse::<u32>().is_err() {
        error!("Due date must be digits of length 4 (e.g. \"1503\" for 15 March)");
        return None
    }
    let (day, month) = (due[..2].parse::<u32>().unwrap(), due[2..].parse::<u32>().unwrap());

    if day > 31 || month > 12 {
        error!("Invalid or out-of-range date");
        return None
    }

    let mut year = Local::now().year();

    // Ensure the due date is in future
    if month < Local::now().month() {
        year += 1;
    }

    if month == Local::now().month() && day < Local::now().day() {
        year += 1;
    }

    let dt = NaiveDate::from_ymd_opt(year, month, day).unwrap().and_hms_opt(12, 0, 0).unwrap();
    dt.timestamp().try_into().ok()
}

/// Start up the preferred editor to edit a task's description.
pub fn prompt_text(task_info: TaskInfo, what: &str) -> Result<Option<String>> {
    // Create a temporary file with some comments inside.
    let mut file_path = env::temp_dir();
    let file_name = format!("tau-{}", Timestamp::current_time().0);
    file_path.push(file_name);
    let mut file = File::create(&file_path)?;

    writeln!(file, "\n# Write your task {what} above this line.")?;
    writeln!(file, "# Lines starting with \"#\" will be removed.")?;
    writeln!(file, "# An empty {what} aborts the operation.")?;
    writeln!(file, "\n# ------------------------ >8 ------------------------")?;
    writeln!(file, "# Do not modify or remove the line above.")?;
    writeln!(file, "# Everything below it will be ignored.")?;
    writeln!(file, "\n{}", taskinfo_table(task_info.clone())?)?;
    writeln!(file, "{}", events_table(task_info.clone())?)?;
    writeln!(file, "{}", comments_table(task_info)?)?;

    // Try $EDITOR, and if not, fallback to xdg-open.
    let editor_argv0 = match env::var("EDITOR") {
        Ok(v) => v,
        Err(_) => "xdg-open".into(),
    };

    if let Err(e) = Command::new(editor_argv0).arg(&file_path).status() {
        error!("Running $EDITOR failed, neither env or xdg-open are available");
        return Err(e.into())
    }

    // Whatever has been written in the temp file will be read here.
    let content = fs::read_to_string(&file_path)?;
    fs::remove_file(&file_path)?;

    let mut lines = vec![];
    for i in content.lines() {
        if !i.starts_with('#') {
            lines.push(i.to_string())
        }
        if i == "# ------------------------ >8 ------------------------" {
            break
        }
    }
    Ok(Some(lines.join("\n")))
}
