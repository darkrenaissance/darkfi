use std::{
    env::{temp_dir, var},
    fs::{self, File},
    io::{self, Read, Write},
    net::SocketAddr,
    ops::Index,
    process::Command,
};

use chrono::{Datelike, Local, NaiveDate, NaiveDateTime};
use log::error;
use prettytable::{cell, format, row, table, Cell, Row, Table};
use rand::distributions::{Alphanumeric, DistString};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use darkfi::{Error, Result};
use structopt::StructOpt;
use structopt_toml::StructOptToml;

pub const CONFIG_FILE: &str = "taud_config.toml";
pub const CONFIG_FILE_CONTENTS: &str = include_str!("../../taud_config.toml");

#[derive(StructOpt, Deserialize, Debug)]
pub enum CliTauSubCommands {
    /// Add a new task
    Add {
        /// Specify task title
        title: Option<String>,
        /// Specify task description
        desc: Option<String>,
        /// Assign task to user
        assign: Option<String>,
        /// Task project (can be hierarchical: crypto.zk)
        project: Option<String>,
        /// Due date in DDMM format: "2202" for 22 Feb
        due: Option<String>,
        /// Project rank single precision decimal real value: 4.8761
        rank: Option<f32>,
    },
    /// Update/Edit an existing task by ID
    Update {
        /// Task ID
        id: u64,
        /// Field's name (ex title)
        key: String,
        /// New value
        value: String,
    },
    /// Set or Get task state
    State {
        /// Task ID
        id: u64,
        /// Set task state
        state: Option<String>,
    },
    /// Set or Get comment for a task
    Comment {
        /// Task ID
        id: u64,
        /// Comment author
        author: Option<String>,
        /// Comment content
        content: Option<String>,
    },
    /// List all tasks
    List {},
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskInfo {
    pub ref_id: String,
    pub id: u32,
    pub title: String,
    pub desc: String,
    pub assign: Vec<String>,
    pub project: Vec<String>,
    pub due: Option<i64>,
    pub rank: f32,
    pub created_at: i64,
    pub events: Vec<Value>,
    pub comments: Vec<Value>,
}

/// Tau cli
#[derive(Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "tau")]
pub struct CliTau {
    /// Increase verbosity
    #[structopt(short, parse(from_occurrences))]
    pub verbose: u8,
    /// JSON-RPC listen URL
    #[structopt(long = "rpc", default_value = "127.0.0.1:11055")]
    pub rpc_listen: SocketAddr,
    /// Sets a custom config file
    #[structopt(short, long)]
    pub config: Option<String>,
    #[structopt(subcommand)]
    pub command: Option<CliTauSubCommands>,
    /// Get task by ID
    pub id: Option<String>,
    /// Search criteria (zero or more)
    #[structopt(multiple = true)]
    pub filters: Vec<String>,
}

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

pub fn set_title() -> Result<String> {
    print!("Title: ");
    io::stdout().flush()?;
    let mut t = String::new();
    io::stdin().read_line(&mut t)?;

    if t.is_empty() {
        error!("You can't have a task without a title");
        return Err(Error::OperationFailed)
    }

    if &t[(t.len() - 1)..] == "\n" {
        t.pop();
    }

    Ok(t)
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

pub fn show_task(task: Value, taskinfo: TaskInfo, current_state: String) -> Result<()> {
    let mut table = table!([Bd => "ref_id", &taskinfo.ref_id],
                                            ["id", &taskinfo.id.to_string()],
                                            [Bd =>"title", &taskinfo.title],
                                            ["desc", &taskinfo.desc],
                                            [Bd =>"assign", get_from_task(task.clone(), "assign")?],
                                            ["project", get_from_task(task.clone(), "project")?],
                                            [Bd =>"due", timestamp_to_date(task["due"].clone(),"date")],
                                            ["rank", &taskinfo.rank.to_string()],
                                            [Bd =>"created_at", timestamp_to_date(task["created_at"].clone(), "datetime")],
                                            ["current_state", &current_state],
                                            [Bd => "comments", get_comments(task.clone())?]);

    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    table.set_titles(row!["Name", "Value"]);

    table.printstd();

    let mut event_table = table!(["events", get_events(task)?]);
    event_table.set_format(*format::consts::FORMAT_NO_COLSEP);

    event_table.printstd();

    Ok(())
}

pub fn get_comments(rep: Value) -> Result<String> {
    let task: Value = serde_json::from_value(rep)?;

    let comments: Vec<Value> = serde_json::from_value(task["comments"].clone())?;
    let mut result = String::new();

    for comment in comments {
        result.push_str(comment["author"].as_str().ok_or(Error::OperationFailed)?);
        result.push_str(": ");
        result.push_str(comment["content"].as_str().ok_or(Error::OperationFailed)?);
        result.push('\n');
    }
    result.pop();

    Ok(result)
}

pub fn get_events(rep: Value) -> Result<String> {
    let task: Value = serde_json::from_value(rep)?;

    let events: Vec<Value> = serde_json::from_value(task["events"].clone())?;
    let mut ev = String::new();

    for event in events {
        ev.push_str("State changed to ");
        ev.push_str(event["action"].as_str().ok_or(Error::OperationFailed)?);
        ev.push_str(" at ");
        ev.push_str(&timestamp_to_date(event["timestamp"].clone(), "datetime"));
        ev.push('\n');
    }
    ev.pop();

    Ok(ev)
}

pub fn timestamp_to_date(timestamp: Value, dt: &str) -> String {
    let timestamp = timestamp.as_i64().unwrap_or(0);

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

pub fn get_from_task(task: Value, value: &str) -> Result<String> {
    let vec_values: Vec<Value> = serde_json::from_value(task[value].clone())?;
    let mut result = String::new();
    for (i, _) in vec_values.iter().enumerate() {
        if !result.is_empty() {
            result.push(',');
        }
        result.push_str(vec_values.index(i).as_str().unwrap());
    }
    Ok(result)
}

// Helper function to check task's state
fn check_task_state(task: &Value, state: &str) -> bool {
    let events = match task["events"].as_array() {
        Some(t) => t.to_owned(),
        None => {
            error!("Value is not an array!");
            vec![]
        }
    };

    let last_state = match events.last() {
        Some(s) => s["action"].as_str().unwrap(),
        None => "open",
    };
    state == last_state
}

fn apply_filter(tasks: Vec<Value>, filter: String) -> Result<Vec<Value>> {
    let filtered_tasks: Vec<Value> = match filter.as_str() {
        "open" => tasks.into_iter().filter(|task| check_task_state(task, "open")).collect(),
        "pause" => tasks.into_iter().filter(|task| check_task_state(task, "pause")).collect(),
        "stop" => tasks.into_iter().filter(|task| check_task_state(task, "stop")).collect(),

        _ if filter.len() == 4 && filter.parse::<u32>().is_ok() => {
            let (month, year) =
                (filter[..2].parse::<u32>().unwrap(), filter[2..].parse::<i32>().unwrap());

            let year = year + 2000;

            tasks
                .into_iter()
                .filter(|task| {
                    let date = task["created_at"].as_i64().unwrap();
                    let task_date = NaiveDateTime::from_timestamp(date, 0).date();
                    let filter_date = NaiveDate::from_ymd(year, month, 1);
                    task_date.month() == filter_date.month() &&
                        task_date.year() == filter_date.year()
                })
                .collect()
        }

        _ if filter.contains("assign:") | filter.contains("project:") => {
            let kv: Vec<&str> = filter.split(':').collect();
            let key = kv[0];
            let value = Value::from(kv[1]);

            tasks
                .into_iter()
                .filter(|task| task[key].as_array().unwrap_or(&vec![]).contains(&value))
                .collect()
        }

        _ if filter.contains("rank>") | filter.contains("rank<") => {
            let kv: Vec<&str> = if filter.contains('>') {
                filter.split('>').collect()
            } else {
                filter.split('<').collect()
            };
            let key = kv[0];
            let value = kv[1].parse::<f32>()?;

            tasks
                .into_iter()
                .filter(|task| {
                    let rank = task[key].as_f64().unwrap_or(0.0) as f32;
                    if filter.contains('>') {
                        rank > value
                    } else {
                        rank < value
                    }
                })
                .collect()
        }

        _ => tasks,
    };

    Ok(filtered_tasks)
}

pub fn list_tasks(rep: Value, filters: Vec<String>) -> Result<()> {
    let mut table = Table::new();
    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    table.set_titles(row!["ID", "Title", "Project", "Assigned", "Due", "Rank"]);

    let mut tasks: Vec<Value> = serde_json::from_value(rep)?;

    for filter in filters {
        // TODO need to use iterator or reference instead of copy
        tasks = apply_filter(tasks, filter)?;
    }

    tasks.sort_by(|a, b| b["rank"].as_f64().partial_cmp(&a["rank"].as_f64()).unwrap());

    let (max_rank, min_rank) = if !tasks.is_empty() {
        (
            serde_json::from_value(tasks[0]["rank"].clone())?,
            serde_json::from_value(tasks[tasks.len() - 1]["rank"].clone())?,
        )
    } else {
        (0.0, 0.0)
    };

    for task in tasks {
        let events: Vec<Value> = serde_json::from_value(task["events"].clone())?;
        let state = match events.last() {
            Some(s) => s["action"].as_str().unwrap(),
            None => "open",
        };

        let rank = task["rank"].as_f64().unwrap_or(0.0) as f32;

        let (max_style, min_style, mid_style, gen_style) = if state == "open" {
            ("bFC", "Fb", "Fc", "")
        } else {
            ("iFYBd", "iFYBd", "iFYBd", "iFYBd")
        };

        table.add_row(Row::new(vec![
            Cell::new(&task["id"].to_string()).style_spec(gen_style),
            Cell::new(task["title"].as_str().unwrap()).style_spec(gen_style),
            Cell::new(&get_from_task(task.clone(), "project")?).style_spec(gen_style),
            Cell::new(&get_from_task(task.clone(), "assign")?).style_spec(gen_style),
            Cell::new(&timestamp_to_date(task["due"].clone(), "date")).style_spec(gen_style),
            if rank == max_rank {
                Cell::new(&rank.to_string()).style_spec(max_style)
            } else if rank == min_rank {
                Cell::new(&rank.to_string()).style_spec(min_style)
            } else {
                Cell::new(&rank.to_string()).style_spec(mid_style)
            },
        ]));
    }
    table.printstd();

    Ok(())
}
