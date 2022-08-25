use std::{process::exit, str::FromStr};

use clap::{Parser, Subcommand};
use log::{error, info};
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use url::Url;

use darkfi::{
    rpc::client::RpcClient,
    util::cli::{get_log_config, get_log_level},
    Result,
};

mod drawdown;
mod filter;
mod primitives;
mod rpc;
mod util;
mod view;

use drawdown::{drawdown, to_naivedate};
use primitives::{task_from_cli, State, TaskEvent};
use util::{desc_in_editor, due_as_timestamp};
use view::{comments_as_string, print_task_info, print_task_list};

const DEFAULT_PATH: &str = "~/tau_exported_tasks";

#[derive(Parser)]
#[clap(name = "tau", version)]
struct Args {
    #[clap(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,

    #[clap(short, long, default_value = "tcp://127.0.0.1:23330")]
    /// taud JSON-RPC endpoint
    endpoint: Url,

    /// Search filters (zero or more)
    filters: Vec<String>,

    #[clap(subcommand)]
    command: Option<TauSubcommand>,
}

#[derive(Subcommand)]
enum TauSubcommand {
    /// Add a new task.
    ///
    /// Quick start:
    ///   Adding a new task named "New task":
    ///     tau add New task
    ///   New task with description:
    ///     tau add Add more info to tau desc:"some awesome description"
    ///   New task with project and assignee:
    ///     tau add Third task project:p2p assign:rusty
    ///   Add a task with due date September 12th and rank of 4.6:
    ///     tau add Task no. Four due:1209 rank:4.6
    ///
    /// Notice that if the command does not have "desc" key it will open
    /// an Editor so you can write the description there.
    ///
    /// Also note that "project" and "assign" keys can have multiple
    /// comma-separated values.
    ///
    /// All keys example:
    ///     tau add Improve CLI desc:"Description here" project:tau,ircd assign:dave,rusty due:0210 rank:2.2
    ///
    #[clap(verbatim_doc_comment)]
    Add {
        /// Pairs of key:value (e.g. desc:description assign:dark).
        values: Vec<String>,
    },

    /// Update/Edit an existing task by ID.
    Update {
        /// Task ID.
        task_id: u64,
        /// Values (e.g. project:blockchain).
        values: Vec<String>,
    },

    /// Set or Get task state.
    State {
        /// Task ID.
        task_id: u64,
        /// Set task state if provided (Get state otherwise).
        state: Option<String>,
    },

    /// Set or Get comment for a task.
    Comment {
        /// Task ID.
        task_id: u64,
        /// Set comment content if provided (Get comments otherwise).
        content: Vec<String>,
    },

    /// Get task info by ID.
    Info { task_id: u64 },

    /// Switch workspace.
    Switch {
        /// Tau workspace.
        workspace: String,
    },

    /// Import tasks from a specified directory.
    Import {
        /// The parent directory from where you want to import tasks.
        path: Option<String>,
    },

    /// Export tasks to a specified directory.
    Export {
        /// The parent directory to where you want to export tasks.
        path: Option<String>,
    },

    /// Log drawdown.
    Log {
        /// The month in which we want to draw a heatmap (e.g. 0822 for August 2022).
        month: String,
        /// The person of which we want to draw a heatmap
        /// (if not provided we list all assignees).
        assignee: Option<String>,
    },
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
                    if let Ok(st) = State::from_str(&state) {
                        tau.set_state(task_id, &st).await
                    } else {
                        error!("State can only be one of the following: open start stop pause",);
                        Ok(())
                    }
                }
                None => {
                    let task = tau.get_task_by_id(task_id).await?;
                    let state = State::from_str(&task.state)?;
                    println!("Task {}: {}", task_id, state);
                    Ok(())
                }
            },

            TauSubcommand::Comment { task_id, content } => {
                if content.is_empty() {
                    let task = tau.get_task_by_id(task_id).await?;
                    let comments = comments_as_string(task.comments);
                    println!("Comments {}:\n{}", task_id, comments);
                    Ok(())
                } else {
                    tau.set_comment(task_id, &content.join(" ")).await
                }
            }

            TauSubcommand::Info { task_id } => {
                let task = tau.get_task_by_id(task_id).await?;
                print_task_info(task)
            }

            TauSubcommand::Switch { workspace } => {
                tau.switch_ws(workspace).await?;
                Ok(())
            }

            TauSubcommand::Export { path } => {
                let path = path.unwrap_or_else(|| DEFAULT_PATH.into());
                let res = tau.export_to(path.clone()).await?;

                if res {
                    info!("Exported to {}", path);
                } else {
                    error!("Error exporting to {}", path);
                }

                Ok(())
            }

            TauSubcommand::Import { path } => {
                let path = path.unwrap_or_else(|| DEFAULT_PATH.into());
                let res = tau.import_from(path.clone()).await?;

                if res {
                    info!("Imported from {}", path);
                } else {
                    error!("Error importing from {}", path);
                }

                Ok(())
            }

            TauSubcommand::Log { month, assignee } => {
                let ts = to_naivedate(month.clone())?.and_hms(12, 0, 0).timestamp();
                let tasks = tau.get_stop_tasks(ts).await?;
                drawdown(month, tasks, assignee)?;

                Ok(())
            }
        },
        None => {
            let ws = tau.get_ws().await?;
            let task_ids = tau.get_ids().await?;
            let mut tasks = vec![];
            for id in task_ids {
                tasks.push(tau.get_task_by_id(id).await?);
            }
            print_task_list(tasks, ws, args.filters)?;
            Ok(())
        }
    }?;

    tau.close_connection().await
}
