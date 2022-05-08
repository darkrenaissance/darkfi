use std::{
    env::{temp_dir, var},
    fs::{self, File},
    io::Read,
    process::Command,
};

use chrono::{Datelike, Local, NaiveDate, NaiveDateTime};
use log::error;
use rand::distributions::{Alphanumeric, DistString};

use darkfi::{Error, Result};

pub const CONFIG_FILE: &str = "taud_config.toml";
pub const CONFIG_FILE_CONTENTS: &str = include_str!("../../taud_config.toml");

pub fn due_as_timestamp(due: &str) -> Option<i64> {
    if due.len() == 4 {
        let (day, month) = (due[..2].parse::<u32>().unwrap(), due[2..].parse::<u32>().unwrap());

        let mut year = Local::today().year();

        if month < Local::today().month() {
            year += 1;
        }

        if month == Local::today().month() && day < Local::today().day() {
            year += 1;
        }

        let dt = NaiveDate::from_ymd(year, month, day).and_hms(12, 0, 0);

        return Some(dt.timestamp())
    }

    if due.len() > 4 {
        error!("due date must be of length 4 (e.g \"1503\" for 15 March)");
    }

    None
}

pub fn desc_in_editor() -> Result<Option<String>> {
    // Create a temporary file with some comments inside
    let mut file_path = temp_dir();
    let file_name = Alphanumeric.sample_string(&mut rand::thread_rng(), 16);
    file_path.push(file_name);
    fs::write(
        &file_path,
        "\n# Write task description above this line\n# These lines will be removed\n",
    )?;

    // Calling env var {EDITOR} on temp file
    let editor = match var("EDITOR") {
        Ok(t) => t,
        Err(e) => {
            error!("EDITOR {}", e);
            return Err(Error::OperationFailed)
        }
    };
    Command::new(editor).arg(&file_path).status()?;

    // Whatever has been written in temp file, will be read here
    let mut lines = String::new();
    File::open(&file_path)?.read_to_string(&mut lines)?;
    fs::remove_file(file_path)?;

    // Store only non-comment lines
    let mut description = String::new();
    for line in lines.split('\n') {
        if !line.starts_with('#') {
            description.push_str(line);
            description.push('\n');
        }
    }
    description.pop();

    Ok(Some(description))
}

pub fn timestamp_to_date(timestamp: i64, dt: &str) -> String {
    if timestamp <= 0 {
        return "".to_string()
    }

    match dt {
        "date" => {
            NaiveDateTime::from_timestamp(timestamp, 0).date().format("%A %-d %B").to_string()
        }
        "datetime" => {
            NaiveDateTime::from_timestamp(timestamp, 0).format("%H:%M %A %-d %B").to_string()
        }
        _ => "".to_string(),
    }
}
