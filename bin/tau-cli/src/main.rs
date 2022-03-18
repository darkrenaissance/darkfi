// TODO: This whole code needs refactoring, clean-up and comments
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
    /// List open tasks
    List {
        /// Month tasks
        #[clap(short, long)]
        month: Option<String>,
    },
    /// Update/Edit an existing task by ID
    Update {
        /// Task ID
        #[clap(short, long)]
        id: Option<u64>,
        /// Data you want to change: "title: new title"
        #[clap(short, long)]
        data: Option<String>,
    },
    /// Set task state
    SetState {
        /// Task ID
        #[clap(short, long)]
        id: u64,
        /// Set task state
        #[clap(short, long)]
        state: String,
    },
    /// Get task state
    GetState {
        /// Task ID
        #[clap(short, long)]
        id: u64,
    },
    /// Set comment for a task
    SetComment {
        /// Task ID
        #[clap(short, long)]
        id: u64,
        /// Comment author
        #[clap(short, long)]
        author: String,
        /// Comment content
        #[clap(short, long)]
        content: String,
    },
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

fn due_as_timestamp(due: Option<String>) -> Option<i64> {
    match due {
        Some(du) => match du.len() {
            0 => None,
            4 => {
                let (day, month) =
                    (du[..2].parse::<u32>().unwrap(), du[2..].parse::<u32>().unwrap());
                let mut year = Local::today().year();
                if month < Local::today().month() {
                    year += 1;
                }
                if month == Local::today().month() && day < Local::today().day() {
                    year += 1;
                }
                let dt = NaiveDate::from_ymd(year, month, day).and_hms(12, 0, 0);

                Some(dt.timestamp())
            }
            _ => {
                error!("due date must be of length 4 (e.g \"1503\" for 15 March)");
                None
            }
        },
        None => None,
    }
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

// List tasks
// --> {"jsonrpc": "2.0", "method": "list", "params": [month_date], "id": 1}
// <-- {"jsonrpc": "2.0", "result": [task, ...], "id": 1}
async fn list(url: &str, month: Option<i64>) -> Result<Value> {
    let req = jsonrpc::request(json!("list"), json!([month]));
    Ok(request(req, url.to_string()).await?)
}

// Update task and returns `true` upon success.
// --> {"jsonrpc": "2.0", "method": "update", "params": [task_id, {"title": "new title"} ], "id": 1}
// <-- {"jsonrpc": "2.0", "result": true, "id": 1}
async fn update(url: &str, id: Option<u64>, data: Value) -> Result<Value> {
    let req = jsonrpc::request(json!("update"), json!([id, data]));
    Ok(request(req, url.to_string()).await?)
}

// Set state for a task and returns `true` upon success.
// --> {"jsonrpc": "2.0", "method": "set_state", "params": [task_id, state], "id": 1}
// <-- {"jsonrpc": "2.0", "result": true, "id": 1}
async fn set_state(url: &str, id: u64, state: &str) -> Result<Value> {
    let req = jsonrpc::request(json!("set_state"), json!([id, state]));
    Ok(request(req, url.to_string()).await?)
}

// Get task's state.
// --> {"jsonrpc": "2.0", "method": "get_state", "params": [task_id], "id": 1}
// <-- {"jsonrpc": "2.0", "result": "state", "id": 1}
async fn get_state(url: &str, id: u64) -> Result<Value> {
    let req = jsonrpc::request(json!("get_state"), json!([id]));
    Ok(request(req, url.to_string()).await?)
}

// Set comment for a task and returns `true` upon success.
// --> {"jsonrpc": "2.0", "method": "set_comment", "params": [task_id, comment_author, comment_content], "id": 1}
// <-- {"jsonrpc": "2.0", "result": true, "id": 1}
async fn set_comment(url: &str, id: u64, author: &str, content: &str) -> Result<Value> {
    let req = jsonrpc::request(json!("set_comment"), json!([id, author, content]));
    Ok(request(req, url.to_string()).await?)
}

async fn start(options: CliTau) -> Result<()> {
    let rpc_addr = "tcp://127.0.0.1:8875";
    match options.command {
        Some(CliTauSubCommands::Add { title, desc, assign, project, due, rank }) => {
            let t = if title.is_none() {
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

            let des = if desc.is_none() {
                let editor = var("EDITOR").unwrap();
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

            // fix this
            let assignee;
            let assigned;
            let a: Option<Vec<&str>> = if assign.is_some() {
                assignee = assign.unwrap();
                assigned = assignee.as_str();
                let somevec = assigned.split(',').collect();

                Some(somevec)
            } else {
                None
            };

            // fix this
            let projecte;
            let projectd;
            let p: Option<Vec<&str>> = if project.is_some() {
                projecte = project.unwrap();
                projectd = projecte.as_str();
                let somevec = projectd.split(',').collect();

                Some(somevec)
            } else {
                None
            };

            let d = due_as_timestamp(due);

            let r = if rank.is_none() { Some(0) } else { rank };

            // Add new task and returns `true` upon success.
            // --> {"jsonrpc": "2.0", "method": "add", "params": ["title", "desc", ["assign"], ["project"], "due", "rank"], "id": 1}
            // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
            let req =
                jsonrpc::request(json!("add"), json!([t.as_deref(), des.as_deref(), a, p, d, r]));
            request(req, rpc_addr.to_string()).await?;

            return Ok(())
        }

        Some(CliTauSubCommands::List { month }) => {
            let ts = if month.is_some() {
                let month = month.unwrap();
                assert!(month.len() == 4);
                let (m, y) = (month[..2].parse::<u32>()?, month[2..].parse::<i32>()?);
                let dt = NaiveDate::from_ymd(y + 2000, m, 1).and_hms(0, 0, 0);

                Some(dt.timestamp())
            } else {
                None
            };

            let mut table = Table::new();
            table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
            table.set_titles(row!["ID", "Title", "Project", "Assigned", "Due", "Rank"]);

            let rep = list(rpc_addr, ts).await?;

            let tasks = rep.as_array().unwrap();
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

                // TODO: sort lines in table by rank
                // TODO: the higher the rank the brighter it is
                table.add_row(row![
                    task["id"],
                    task["title"].as_str().unwrap(),
                    projects,
                    asgn,
                    date,
                    Fb->task["rank"]
                ]);
            }
            table.printstd();

            return Ok(())
        }

        Some(CliTauSubCommands::Update { id, data }) => {
            let data = data.unwrap();
            let kv: Vec<&str> = data.as_str().split(':').collect();

            let new_data = match kv[0].trim() {
                "title" | "description" => {
                    json!({kv[0].trim(): kv[1].trim()})
                }
                "due" => {
                    let parsed_data: Option<i64> = due_as_timestamp(Some(kv[1].trim().to_string()));
                    json!({ kv[0].trim(): parsed_data })
                }
                "rank" => {
                    let parsed_data: Option<u64> = Some(kv[1].trim().parse()?);
                    json!({ kv[0].trim(): parsed_data })
                }
                _ => {
                    let parsed_data: Vec<&str> = kv[1].trim().split(',').collect();
                    json!({ kv[0].trim(): parsed_data })
                }
            };

            update(rpc_addr, id, new_data).await?;

            return Ok(())
        }

        Some(CliTauSubCommands::SetState { id, state }) => {
            set_state(rpc_addr, id, state.trim()).await?;

            return Ok(())
        }

        Some(CliTauSubCommands::GetState { id }) => {
            let state = get_state(rpc_addr, id).await?;
            println!("Task with id: {} is {}", id, state);

            return Ok(())
        }

        Some(CliTauSubCommands::SetComment { id, author, content }) => {
            set_comment(rpc_addr, id, author.trim(), content.trim()).await?;

            return Ok(())
        }
        _ => (),
    }
    error!("Please run 'tau help' to see usage.");

    Err(Error::MissingParams)
}

#[async_std::main]
async fn main() -> Result<()> {
    let args = CliTau::parse();
    let matches = CliTau::command().get_matches();
    let verbosity_level = matches.occurrences_of("verbose");

    //let config_path = if args.config.is_some() {
    //    expand_path(&args.config.clone().unwrap())?
    //} else {
    //    join_config_path(&PathBuf::from("tau.toml"))?
    //};

    // Spawn config file if it's not in place already.
    //spawn_config(&config_path, CONFIG_FILE_CONTENTS)?;

    let (lvl, conf) = log_config(verbosity_level)?;
    TermLogger::init(lvl, conf, TerminalMode::Mixed, ColorChoice::Auto)?;

    start(args).await
}
