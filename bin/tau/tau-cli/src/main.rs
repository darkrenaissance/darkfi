use log::error;
use serde_json::json;
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use structopt_toml::StructOptToml;
use url::Url;

use darkfi::{
    rpc::rpcclient::RpcClient,
    util::{
        cli::{log_config, spawn_config},
        path::get_config_path,
    },
    Result,
};

mod cli;
mod filter;
mod jsonrpc;
mod primitives;
mod util;
mod view;

use cli::CliTauSubCommands;
use jsonrpc::Rpc;
use primitives::{TaskEvent, TaskInfo};
use util::{desc_in_editor, CONFIG_FILE, CONFIG_FILE_CONTENTS};
use view::{comments_as_string, print_list_of_task, print_task_info};

async fn start(mut options: cli::CliTau) -> Result<()> {
    let rpc_client = Rpc { client: RpcClient::new(Url::parse(&options.rpc_listen)?).await? };

    let states: Vec<String> = vec!["stop".into(), "open".into(), "pause".into()];

    match options.id {
        Some(id) if id.len() < 4 && id.parse::<u64>().is_ok() => {
            let task = rpc_client.get_task_by_id(id.parse::<u64>().unwrap()).await?;
            let taskinfo: TaskInfo = serde_json::from_value(task.clone())?;
            print_task_info(taskinfo)?;
            return Ok(())
        }
        Some(id) => options.filters.push(id),
        None => {}
    }

    match options.command {
        Some(CliTauSubCommands::Add { values }) => {
            let mut task = cli::task_from_cli_values(values)?;
            if task.title.is_empty() {
                error!("Provide a title for the task");
                return Ok(())
            };

            if task.desc.is_none() {
                task.desc = desc_in_editor()?;
            };

            rpc_client.add(json!([task])).await?;
        }

        Some(CliTauSubCommands::Update { id, values }) => {
            let task = cli::task_from_cli_values(values)?;
            rpc_client.update(id, json!([task])).await?;
        }

        Some(CliTauSubCommands::State { id, state }) => match state {
            Some(state) => {
                let state = state.trim().to_lowercase();
                if states.contains(&state) {
                    rpc_client.set_state(id, &state).await?;
                } else {
                    error!("Task state could only be one of three states: open, pause or stop");
                }
            }
            None => {
                let task = rpc_client.get_task_by_id(id).await?;
                let taskinfo: TaskInfo = serde_json::from_value(task.clone())?;
                let default_event = TaskEvent::default();
                let state = &taskinfo.events.last().unwrap_or(&default_event).action;
                println!("Task {}: {}", id, state);
            }
        },

        Some(CliTauSubCommands::Comment { id, content }) => match content {
            Some(content) => {
                rpc_client.set_comment(id, content.trim()).await?;
            }
            None => {
                let task = rpc_client.get_task_by_id(id).await?;
                let taskinfo: TaskInfo = serde_json::from_value(task.clone())?;
                let comments = comments_as_string(taskinfo.comments);
                println!("Comments {}:\n{}", id, comments);
            }
        },

        Some(CliTauSubCommands::List {}) | None => {
            let task_ids = rpc_client.get_ids(json!([])).await?;
            let mut tasks: Vec<TaskInfo> = vec![];
            if let Some(ids) = task_ids.as_array() {
                for id in ids {
                    let id = if id.is_u64() { id.as_u64().unwrap() } else { continue };
                    let task = rpc_client.get_task_by_id(id).await?;
                    let taskinfo: TaskInfo = serde_json::from_value(task.clone())?;
                    tasks.push(taskinfo);
                }
            }

            // let mut tasks: Vec<TaskInfo> = serde_json::from_value(tasks)?;
            print_list_of_task(&mut tasks, options.filters)?;
        }
    }

    Ok(())
}

#[async_std::main]
async fn main() -> Result<()> {
    let args = cli::CliTau::from_args_with_toml("").unwrap();
    let cfg_path = get_config_path(args.config, CONFIG_FILE)?;
    spawn_config(&cfg_path, CONFIG_FILE_CONTENTS.as_bytes())?;
    let args = cli::CliTau::from_args_with_toml(&std::fs::read_to_string(cfg_path)?).unwrap();

    let (lvl, conf) = log_config(args.verbose.into())?;
    TermLogger::init(lvl, conf, TerminalMode::Mixed, ColorChoice::Auto)?;

    start(args).await
}
