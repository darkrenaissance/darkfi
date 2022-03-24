use std::{
    env::{temp_dir, var},
    fs::{self, File},
    io,
    io::{Read, Write},
    ops::Index,
    process::Command,
};

use chrono::{Datelike, Local, NaiveDate, NaiveDateTime};
use clap::{CommandFactory, Parser, Subcommand};
use log::{debug, error};
use prettytable::{cell, format, row, Table};
use serde_json::{json, Value};
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use url::Url;

use darkfi::{
    rpc::jsonrpc::{self, JsonResult},
    util::cli::log_config,
    Error, Result,
};

#[derive(Subcommand)]
pub enum CliTauSubCommands {
    /// Add a new task
    Add {
        /// Specify task title
        #[clap(short, long)]
        title: Option<String>,
        /// Specify task description
        #[clap(long)]
        desc: Option<String>,
        /// Assign task to user
        #[clap(short, long)]
        assign: Option<String>,
        /// Task project (can be hierarchical: crypto.zk)
        #[clap(short, long)]
        project: Option<String>,
        /// Due date in DDMM format: "2202" for 22 Feb
        #[clap(short, long)]
        due: Option<String>,
        /// Project rank
        #[clap(short, long)]
        rank: Option<u32>,
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
    /// Set task state
    SetState {
        /// Task ID
        id: u64,
        /// Set task state
        state: String,
    },
    /// Get task state
    GetState {
        /// Task ID
        id: u64,
    },
    /// Set comment for a task
    SetComment {
        /// Task ID
        id: u64,
        /// Comment author
        author: String,
        /// Comment content
        content: String,
    },
    /// Get task's comments
    GetComment {
        /// Task ID
        id: u64,
    },
    /// List open tasks
    List {},
}

/// Tau cli
#[derive(Parser)]
#[clap(name = "tau")]
#[clap(author, version, about)]
pub struct CliTau {
    /// Increase verbosity
    #[clap(short, parse(from_occurrences))]
    pub verbose: u8,
    #[clap(subcommand)]
    pub command: Option<CliTauSubCommands>,
}

fn due_as_timestamp(due: &str) -> Option<i64> {
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

async fn request(r: jsonrpc::JsonRequest, url: String) -> Result<Value> {
    let reply: JsonResult = match jsonrpc::send_request(&Url::parse(&url)?, json!(r), None).await {
        Ok(v) => v,
        Err(e) => return Err(e),
    };

    match reply {
        JsonResult::Resp(r) => {
            debug!(target: "RPC", "<-- {}", serde_json::to_string(&r)?);
            Ok(r.result)
        }

        JsonResult::Err(e) => {
            debug!(target: "RPC", "<-- {}", serde_json::to_string(&e)?);
            Err(Error::JsonRpcError(e.error.message.to_string()))
        }

        JsonResult::Notif(n) => {
            debug!(target: "RPC", "<-- {}", serde_json::to_string(&n)?);
            Err(Error::JsonRpcError("Unexpected reply".to_string()))
        }
    }
}

// Add new task and returns `true` upon success.
// --> {"jsonrpc": "2.0", "method": "add", "params": ["title", "desc", ["assign"], ["project"], "due", "rank"], "id": 1}
// <-- {"jsonrpc": "2.0", "result": true, "id": 1}
async fn add(url: &str, params: Value) -> Result<Value> {
    let req = jsonrpc::request(json!("add"), params);
    request(req, url.to_string()).await
}

// List tasks
// --> {"jsonrpc": "2.0", "method": "list", "params": [], "id": 1}
// <-- {"jsonrpc": "2.0", "result": [task, ...], "id": 1}
async fn list(url: &str, params: Value) -> Result<Value> {
    let req = jsonrpc::request(json!("list"), json!(params));
    request(req, url.to_string()).await
}

// Update task and returns `true` upon success.
// --> {"jsonrpc": "2.0", "method": "update", "params": [task_id, {"title": "new title"} ], "id": 1}
// <-- {"jsonrpc": "2.0", "result": true, "id": 1}
async fn update(url: &str, id: u64, data: Value) -> Result<Value> {
    let req = jsonrpc::request(json!("update"), json!([id, data]));
    request(req, url.to_string()).await
}

// Set state for a task and returns `true` upon success.
// --> {"jsonrpc": "2.0", "method": "set_state", "params": [task_id, state], "id": 1}
// <-- {"jsonrpc": "2.0", "result": true, "id": 1}
async fn set_state(url: &str, id: u64, state: &str) -> Result<Value> {
    let req = jsonrpc::request(json!("set_state"), json!([id, state]));
    request(req, url.to_string()).await
}

// Get task's state.
// --> {"jsonrpc": "2.0", "method": "get_state", "params": [task_id], "id": 1}
// <-- {"jsonrpc": "2.0", "result": "state", "id": 1}
async fn get_state(url: &str, id: u64) -> Result<Value> {
    let req = jsonrpc::request(json!("get_state"), json!([id]));
    request(req, url.to_string()).await
}

// Set comment for a task and returns `true` upon success.
// --> {"jsonrpc": "2.0", "method": "set_comment", "params": [task_id, comment_author, comment_content], "id": 1}
// <-- {"jsonrpc": "2.0", "result": true, "id": 1}
async fn set_comment(url: &str, id: u64, author: &str, content: &str) -> Result<Value> {
    let req = jsonrpc::request(json!("set_comment"), json!([id, author, content]));
    request(req, url.to_string()).await
}

async fn start(options: CliTau) -> Result<()> {
    let rpc_addr = "tcp://127.0.0.1:8875";
    match options.command {
        Some(CliTauSubCommands::Add { title, desc, assign, project, due, rank }) => {
            let title = if title.is_none() {
                print!("Title: ");
                io::stdout().flush()?;
                let mut t = String::new();
                io::stdin().read_line(&mut t)?;
                if &t[(t.len() - 1)..] == "\n" {
                    t.pop();
                }
                Some(t)
            } else {
                title
            };

            let desc = if desc.is_none() {
                let editor = match var("EDITOR") {
                    Ok(t) => t,
                    Err(e) => {
                        error!("EDITOR {}", e);
                        return Err(Error::BadOperationType)
                    }
                };
                let mut file_path = temp_dir();
                file_path.push("temp_file");
                File::create(&file_path)?;
                fs::write(
                    &file_path,
                    "\n# Write task description above this line\n# These lines will be removed\n",
                )?;

                Command::new(editor).arg(&file_path).status()?;

                let mut lines = String::new();
                File::open(file_path)?.read_to_string(&mut lines)?;

                let mut description = String::new();
                for line in lines.split('\n') {
                    if !line.starts_with('#') {
                        description.push_str(line)
                    }
                }

                Some(description)
            } else {
                desc
            };

            let assign: Vec<String> = match assign {
                Some(a) => a.split(',').map(|s| s.into()).collect(),
                None => vec![],
            };

            let project: Vec<String> = match project {
                Some(p) => p.split(',').map(|s| s.into()).collect(),
                None => vec![],
            };

            let due = match due {
                Some(d) => due_as_timestamp(&d),
                None => None,
            };

            let rank = rank.unwrap_or(0);

            add(rpc_addr, json!([title, desc, assign, project, due, rank])).await?;
        }

        Some(CliTauSubCommands::List {}) => {
            let rep = list(rpc_addr, json!([])).await?;

            let mut table = Table::new();
            table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
            table.set_titles(row!["ID", "Title", "Project", "Assigned", "Due", "Rank"]);

            let mut tasks = rep.as_array().unwrap().to_owned();
            tasks.sort_by(|a, b| b["rank"].as_u64().cmp(&a["rank"].as_u64()));

            let max_rank = if !tasks.is_empty() { tasks[0]["rank"].as_u64().unwrap() } else { 0 };

            for task in tasks {
                let project = task["project"].as_array().unwrap();
                let mut projects = String::new();
                for (i, _) in project.iter().enumerate() {
                    if !projects.is_empty() {
                        projects.push(',');
                    }
                    projects.push_str(project.index(i).as_str().unwrap());
                }

                let assign = task["assign"].as_array().unwrap();
                let mut asgn = String::new();
                for (i, _) in assign.iter().enumerate() {
                    if !asgn.is_empty() {
                        asgn.push(',');
                    }
                    asgn.push_str(assign.index(i).as_str().unwrap());
                }

                let date = if task["due"].is_u64() {
                    let due = task["due"].as_i64().unwrap();
                    NaiveDateTime::from_timestamp(due, 0).date().format("%A %-d %B").to_string()
                } else {
                    "".to_string()
                };

                if task["rank"].as_u64().unwrap() == max_rank {
                    table.add_row(row![
                        task["id"],
                        task["title"].as_str().unwrap(),
                        projects,
                        asgn,
                        date,
                        bFC->task["rank"]
                    ]);
                } else {
                    table.add_row(row![
                        task["id"],
                        task["title"].as_str().unwrap(),
                        projects,
                        asgn,
                        date,
                        Fb->task["rank"]
                    ]);
                }
            }
            table.printstd();
        }

        Some(CliTauSubCommands::Update { id, key, value }) => {
            let value = value.as_str().trim();

            let updated_value: Value = match key.as_str() {
                "due" => {
                    json!(due_as_timestamp(value))
                }
                "rank" => {
                    json!(value.parse::<u64>()?)
                }
                "project" | "assign" => {
                    json!(value.split(',').collect::<Vec<&str>>())
                }
                _ => {
                    json!(value)
                }
            };

            update(rpc_addr, id, json!({ key: updated_value })).await?;
        }

        Some(CliTauSubCommands::SetState { id, state }) => {
            set_state(rpc_addr, id, state.trim()).await?;
        }

        Some(CliTauSubCommands::GetState { id }) => {
            let state = get_state(rpc_addr, id).await?;
            println!("Task with id: {} is {}", id, state);
        }

        Some(CliTauSubCommands::SetComment { id, author, content }) => {
            set_comment(rpc_addr, id, author.trim(), content.trim()).await?;
        }

        Some(CliTauSubCommands::GetComment { id }) => {
            let rep = list(rpc_addr, json!([])).await?;
            let tasks = rep.as_array().unwrap();

            if tasks.iter().any(|x| x["id"].as_u64().unwrap() == id) {
                let index: usize = (id - 1).try_into().unwrap();
                let comments = tasks[index]["comments"].as_array().unwrap();
                let mut cmnt = String::new();

                for comment in comments {
                    cmnt.push_str(comment["author"].as_str().unwrap());
                    cmnt.push_str(": ");
                    cmnt.push_str(comment["content"].as_str().unwrap());
                    cmnt.push('\n');
                }

                println!("Comments on Task with id {}:\n{}", id, cmnt);
            }
        }

        _ => {
            error!("Please run 'tau help' to see usage.");
            return Err(Error::MissingParams)
        }
    }
    Ok(())
}

#[async_std::main]
async fn main() -> Result<()> {
    let args = CliTau::parse();
    let matches = CliTau::command().get_matches();
    let verbosity_level = matches.occurrences_of("verbose");

    let (lvl, conf) = log_config(verbosity_level)?;
    TermLogger::init(lvl, conf, TerminalMode::Mixed, ColorChoice::Auto)?;

    start(args).await
}
