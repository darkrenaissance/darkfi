use std::process::exit;

use clap::{Parser, Subcommand};
use log::error;
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use url::Url;

use darkfi::{cli_desc, rpc::client::RpcClient, util::cli::log_config, Error, Result};

mod filter;
mod primitives;
mod rpc;
mod util;
mod view;

use primitives::{task_from_cli, TaskEvent};
use util::{desc_in_editor, due_as_timestamp};
use view::{comments_as_string, print_task_info, print_task_list};

#[derive(Parser)]
#[clap(name = "tau", about = cli_desc!(), version)]
#[clap(arg_required_else_help(true))]
struct Args {
    #[clap(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,

    #[clap(short, long, default_value = "tcp://127.0.0.1:11055")]
    /// taud JSON-RPC endpoint
    endpoint: Url,

    #[clap(subcommand)]
    command: TauSubcommand,
}

#[derive(Subcommand)]
enum TauSubcommand {
    /// Add a new task
    Add { values: Vec<String> },

    /// Update/Edit an existing task by ID
    Update {
        /// Task ID
        id: u64,
        /// Values (ex: project:blockchain)
        values: Vec<String>,
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
        /// Comment content
        content: Option<String>,
    },

    /// List all tasks
    List {
        /// Search criteria (zero or more)
        filters: Vec<String>,
    },

    /// Get task info by ID
    Info { id: u64 },
}

pub struct Tau {
    pub rpc_client: RpcClient,
}

#[async_std::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let (lvl, conf) = log_config(args.verbose.into())?;
    TermLogger::init(lvl, conf, TerminalMode::Mixed, ColorChoice::Auto)?;

    let rpc_client = RpcClient::new(args.endpoint).await?;
    let tau = Tau { rpc_client };

    // Allowed states for a task
    let states = ["stop", "open", "pause"];

    // Parse subcommands
    match args.command {
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

        TauSubcommand::Update { id, values } => {
            let task = task_from_cli(values)?;
            tau.update(id, task).await
        }

        TauSubcommand::State { id, state } => match state {
            Some(state) => {
                let state = state.trim().to_lowercase();
                if states.contains(&state.as_str()) {
                    tau.set_state(id, &state).await
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
                let task = tau.get_task_by_id(id).await?;
                let state = &task.events.last().unwrap_or(&TaskEvent::default()).action.clone();
                println!("Task {}: {}", id, state);
                Ok(())
            }
        },

        TauSubcommand::Comment { id, content } => match content {
            Some(content) => tau.set_comment(id, content.trim()).await,
            None => {
                let task = tau.get_task_by_id(id).await?;
                let comments = comments_as_string(task.comments);
                println!("Comments {}:\n{}", id, comments);
                Ok(())
            }
        },

        TauSubcommand::List { filters } => {
            let task_ids = tau.get_ids().await?;
            let mut tasks = vec![];
            for id in task_ids {
                tasks.push(tau.get_task_by_id(id).await?);
            }
            print_task_list(tasks, filters)?;
            Ok(())
        }

        TauSubcommand::Info { id } => {
            let task = tau.get_task_by_id(id).await?;
            print_task_info(task)?;
            Ok(())
        }
    }?;

    tau.close_connection().await
}
