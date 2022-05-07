use log::error;
use serde_json::{json, Value};
use simplelog::{ColorChoice, TermLogger, TerminalMode};

use darkfi::{
    util::{
        cli::{log_config, spawn_config},
        path::get_config_path,
    },
    Result,
};
use structopt_toml::StructOptToml;

mod jsonrpc;
mod util;

use crate::{
    jsonrpc::{add, get_by_id, get_state, list, set_comment, set_state, update},
    util::{
        desc_in_editor, due_as_timestamp, get_comments, list_tasks, set_title, show_task, CliTau,
        CliTauSubCommands, TaskInfo, CONFIG_FILE, CONFIG_FILE_CONTENTS,
    },
};

async fn start(mut options: CliTau) -> Result<()> {
    let rpc_addr = &format!("tcp://{}", &options.rpc_listen.clone());

    match options.id {
        Some(id) if id.len() < 4 && id.parse::<u64>().is_ok() => {
            let task = get_by_id(rpc_addr, id.parse::<u64>().unwrap()).await?;
            let taskinfo: TaskInfo = serde_json::from_value(task.clone())?;
            let current_state: String =
                serde_json::from_value(get_state(rpc_addr, id.parse::<u64>().unwrap()).await?)?;
            show_task(task, taskinfo, current_state)?;
            return Ok(())
        }
        Some(id) => options.filters.push(id),
        None => {}
    }

    match options.command {
        Some(CliTauSubCommands::Add { title, desc, assign, project, due, rank }) => {
            let title = match title {
                Some(t) => t,
                None => set_title()?,
            };

            let desc = match desc {
                Some(d) => Some(d),
                None => desc_in_editor()?,
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

            let rank = rank.unwrap_or(0.0);

            add(rpc_addr,
                json!([{"title": title, "desc": desc, "assign": assign, "project": project, "due": due, "rank": rank}]),
                ).await?;
        }

        Some(CliTauSubCommands::Update { id, key, value }) => {
            let value = value.as_str().trim();

            let updated_value: Value = match key.as_str() {
                "due" => {
                    json!(due_as_timestamp(value))
                }
                "rank" => {
                    json!(value.parse::<f32>()?)
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

        Some(CliTauSubCommands::State { id, state }) => match state {
            Some(state) => {
                if state.as_str() == "open" {
                    set_state(rpc_addr, id, state.trim()).await?;
                } else if state.as_str() == "pause" {
                    set_state(rpc_addr, id, state.trim()).await?;
                } else if state.as_str() == "stop" {
                    set_state(rpc_addr, id, state.trim()).await?;
                } else {
                    error!("Task state could only be one of three states: open, pause or stop");
                }
            }
            None => {
                let state = get_state(rpc_addr, id).await?;
                println!("Task with id {} is: {}", id, state);
            }
        },

        Some(CliTauSubCommands::Comment { id, content }) => match content {
            Some(content) => {
                set_comment(rpc_addr, id, content.trim()).await?;
            }
            None => {
                let rep = get_by_id(rpc_addr, id).await?;
                let comments = get_comments(rep)?;

                println!("Comments on Task with id {}:\n{}", id, comments);
            }
        },

        Some(CliTauSubCommands::List {}) | None => {
            let rep = list(rpc_addr, json!([])).await?;
            list_tasks(rep, options.filters)?;
        }
    }

    Ok(())
}

#[async_std::main]
async fn main() -> Result<()> {
    let args = CliTau::from_args_with_toml("").unwrap();
    let cfg_path = get_config_path(args.config, CONFIG_FILE)?;
    spawn_config(&cfg_path, CONFIG_FILE_CONTENTS.as_bytes())?;
    let args = CliTau::from_args_with_toml(&std::fs::read_to_string(cfg_path)?).unwrap();

    let (lvl, conf) = log_config(args.verbose.into())?;
    TermLogger::init(lvl, conf, TerminalMode::Mixed, ColorChoice::Auto)?;

    start(args).await
}
