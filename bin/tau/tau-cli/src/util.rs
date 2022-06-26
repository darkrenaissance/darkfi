use std::{
    env,
    fs::{self, File},
    io::Write,
    process::Command,
};

use chrono::{Datelike, Local, NaiveDate};
use log::error;

use darkfi::{util::Timestamp, Result};

/// Parse due date (e.g. "1503" for 15 March) as i64 timestamp.
pub fn due_as_timestamp(due: &str) -> Option<i64> {
    if due.len() != 4 || !due.parse::<u32>().is_ok() {
        error!("Due date must be digits of length 4 (e.g. \"1503\" for 15 March)");
        return None
    }
    let (day, month) = (due[..2].parse::<u32>().unwrap(), due[2..].parse::<u32>().unwrap());

    if day > 31 || month > 12 {
        error!("Invalid or out-of-range date");
        return None
    }

    let mut year = Local::today().year();

    // Ensure the due date is in future
    if month < Local::today().month() {
        year += 1;
    }

    if month == Local::today().month() && day < Local::today().day() {
        year += 1;
    }

    let dt = NaiveDate::from_ymd(year, month, day).and_hms(12, 0, 0);
    Some(dt.timestamp())
}

/// Start up the preferred editor to edit a task's description.
pub fn desc_in_editor() -> Result<Option<String>> {
    // Create a temporary file with some comments inside.
    let mut file_path = env::temp_dir();
    let file_name = format!("tau-{}", Timestamp::current_time().0);
    file_path.push(file_name);
    let mut file = File::create(&file_path)?;

    writeln!(file, "\n# Write your task description here.")?;
    writeln!(file, "# Lines starting with \"#\" will be removed")?;

    // Try $EDITOR, and if not, fallback to xdg-open.
    let editor_argv0 = match env::var("EDITOR") {
        Ok(v) => v,
        Err(_) => "xdg-open".into(),
    };

    Command::new(editor_argv0).arg(&file_path).status()?;

    // Whatever has been written in the temp file will be read here.
    let content = fs::read_to_string(&file_path)?;
    fs::remove_file(&file_path)?;

    let mut lines = vec![];
    for i in content.lines() {
        if !i.starts_with('#') {
            lines.push(i.to_string())
        }
    }
    Ok(Some(lines.join("\n")))
}
