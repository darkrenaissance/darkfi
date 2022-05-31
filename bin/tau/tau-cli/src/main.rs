use std::process::exit;

use clap::{Parser, Subcommand};
use log::error;
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use url::Url;

use darkfi::{
    rpc::client::RpcClient,
    util::cli::{get_log_config, get_log_level},
    Error, Result,
};

mod filter;
mod primitives;
mod rpc;
mod util;
mod view;

use primitives::{task_from_cli, TaskEvent};
use util::{desc_in_editor, due_as_timestamp};
use view::{comments_as_string, print_task_info, print_task_list};

#[derive(Parser)]
#[clap(name = "tau", version)]
struct Args {
    #[clap(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,

    #[clap(short, long, default_value = "tcp://127.0.0.1:11055")]
    /// taud JSON-RPC endpoint
    endpoint: Url,

    /// Search filters (zero or more)
    filters: Vec<String>,

    #[clap(subcommand)]
    command: Option<TauSubcommand>,
}

#[derive(Subcommand)]
enum TauSubcommand {
    /// Add a new task
    Add { values: Vec<String> },

    /// Update/Edit an existing task by ID
    Update {
        /// Task ID
        task_id: u64,
        /// Values (ex: project:blockchain)
        values: Vec<String>,
    },

    /// Set or Get task state
    State {
        /// Task ID
        task_id: u64,
        /// Set task state
        state: Option<String>,
    },

    /// Set or Get comment for a task
    Comment {
        /// Task ID
        task_id: u64,
        /// Comment content
        content: Option<String>,
    },

    /// Get task info by ID
    Info { task_id: u64 },
}

pub struct Tau {
    pub rpc_client: RpcClient,
}

#[async_std::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let log_level = get_log_level(args.verbose.into());
    let log_config = get_log_config();
    TermLogger::init(log_level, log_config, TerminalMode::Mixed, ColorChoice::Auto)?;

    let rpc_client = RpcClient::new(args.endpoint).await?;
    let tau = Tau { rpc_client };

    // Allowed states for a task
    let states = ["stop", "open", "pause"];

    // Parse subcommands
    match args.command {
        Some(sc) => match sc {
            TauSubcommand::Add { values } => {
                let mut task = task_from_cli(values)?;
                if task.title.is_empty() {
                    error!("Please provide a title for the task.");
                    exit(1);
                };

                if task.desc.is_none() {
                    task.desc = desc_in_editor()?;
                };

                return tau.add(task).await
            }

            TauSubcommand::Update { task_id, values } => {
                let task = task_from_cli(values)?;
                tau.update(task_id, task).await
            }

            TauSubcommand::State { task_id, state } => match state {
                Some(state) => {
                    let state = state.trim().to_lowercase();
                    if states.contains(&state.as_str()) {
                        tau.set_state(task_id, &state).await
                    } else {
                        error!(
                            "Task state can only be one of the following {}: {:?}",
                            states.len(),
                            states
                        );
                        return Err(Error::OperationFailed)
                    }
                }
                None => {
                    let task = tau.get_task_by_id(task_id).await?;
                    let state = &task.events.last().unwrap_or(&TaskEvent::default()).action.clone();
                    println!("Task {}: {}", task_id, state);
                    Ok(())
                }
            },

            TauSubcommand::Comment { task_id, content } => match content {
                Some(content) => tau.set_comment(task_id, content.trim()).await,
                None => {
                    let task = tau.get_task_by_id(task_id).await?;
                    let comments = comments_as_string(task.comments);
                    println!("Comments {}:\n{}", task_id, comments);
                    Ok(())
                }
            },

            TauSubcommand::Info { task_id } => {
                let task = tau.get_task_by_id(task_id).await?;
                print_task_info(task)
            }
        },
        None => {
            let task_ids = tau.get_ids().await?;
            let mut tasks = vec![];
            for id in task_ids {
                tasks.push(tau.get_task_by_id(id).await?);
            }
            print_task_list(tasks, args.filters)?;
            Ok(())
        }
    }?;

    tau.close_connection().await
}
