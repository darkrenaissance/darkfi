/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::process::exit;

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
use filter::{apply_filter, get_ids, no_filter_warn};
use primitives::{task_from_cli, State, TaskEvent};
use util::{due_as_timestamp, prompt_text};
use view::{print_task_info, print_task_list};

use crate::primitives::TaskInfo;

const DEFAULT_PATH: &str = "~/tau_exported_tasks";

#[derive(Parser)]
#[clap(name = "tau", version)]
#[clap(subcommand_precedence_over_arg = true)]
struct Args {
    #[arg(short, action = clap::ArgAction::Count)]
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
    ///     tau add Third task project:p2p Arusty
    ///   Add a task with due date September 12th and rank of 4.6:
    ///     tau add Task no. Four due:1209 rank:4.6
    ///
    /// Notice that if the command does not have "desc" key it will open
    /// an Editor so you can write the description there.
    ///
    /// Also note that "project" key can have multiple
    /// comma-separated values.
    /// "assign" on the other hand uses '@' character but also could be
    /// multiple values, but like so:
    /// @person1 @person2
    ///
    /// All keys example:
    ///     tau add Improve CLI desc:"Description here" project:tau,ircd @dave @rusty due:0210 rank:2.2
    ///
    #[clap(verbatim_doc_comment)]
    Add {
        /// Pairs of key:value (e.g. desc:description @dark).
        values: Vec<String>,
    },

    /// Modify/Edit an existing task.
    Modify {
        #[clap(allow_hyphen_values = true)]
        /// Values (e.g. project:blockchain).
        values: Vec<String>,
    },

    /// List tasks.
    List,

    /// Start task(s).
    Start,

    /// Open task(s).
    Open,

    /// Pause task(s).
    Pause,

    /// Stop task(s).
    Stop,

    /// Set or Get comment for task(s).
    Comment {
        /// Set comment content if provided (Get comments otherwise).
        content: Vec<String>,
    },

    /// Get all data about selected task(s).
    Info,

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
        month: Option<String>,
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

    let mut filters = args.filters.clone();

    // If IDs are provided in filter we use them to get the tasks from the daemon
    // then remove IDs from filter so we can do apply_filter() normally.
    // If not provided we use get_ids() to get them from the daemon.
    let ids = get_ids(&mut filters)?;
    let task_ids = if ids.is_empty() { tau.get_ids().await? } else { ids };

    let mut tasks =
        if filters.contains(&"state:stop".to_string()) || filters.contains(&"all".to_string()) {
            tau.get_stop_tasks(None).await?
        } else {
            vec![]
        };
    for id in task_ids {
        tasks.push(tau.get_task_by_id(id).await?);
    }

    for filter in filters {
        apply_filter(&mut tasks, &filter);
    }

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
                    task.desc = prompt_text(TaskInfo::from(task.clone()), "description")?;
                };

                if task.clone().desc.unwrap().trim().is_empty() {
                    error!("Abort adding the task due to empty description.");
                    exit(1)
                }

                let title = task.clone().title;
                if tau.add(task).await? {
                    println!("Created task \"{}\"", title);
                }
                Ok(())
            }

            TauSubcommand::Modify { values } => {
                if args.filters.is_empty() {
                    no_filter_warn()
                }
                let base_task = task_from_cli(values)?;
                for task in tasks.clone() {
                    let res = tau.update(task.id.into(), base_task.clone()).await?;
                    if res {
                        let tsk = tau.get_task_by_id(task.id.into()).await?;
                        print_task_info(tsk)?;
                        println!()
                    }
                }

                Ok(())
            }

            TauSubcommand::Start => {
                if args.filters.is_empty() {
                    no_filter_warn()
                }
                let state = State::Start;
                for task in tasks {
                    if tau.set_state(task.id.into(), &state).await? {
                        println!("Started task: {:?}", task.id);
                    }
                }

                Ok(())
            }

            TauSubcommand::Open => {
                if args.filters.is_empty() {
                    no_filter_warn()
                }
                let state = State::Open;
                for task in tasks {
                    if tau.set_state(task.id.into(), &state).await? {
                        println!("Opened task: {:?}", task.id);
                    }
                }

                Ok(())
            }

            TauSubcommand::Pause => {
                if args.filters.is_empty() {
                    no_filter_warn()
                }
                let state = State::Pause;
                for task in tasks {
                    if tau.set_state(task.id.into(), &state).await? {
                        println!("Paused task: {:?}", task.id);
                    }
                }

                Ok(())
            }

            TauSubcommand::Stop => {
                if args.filters.is_empty() {
                    no_filter_warn()
                }
                let state = State::Stop;
                for task in tasks {
                    if tau.set_state(task.id.into(), &state).await? {
                        println!("Stopped task: {}", task.id);
                    }
                }

                Ok(())
            }

            TauSubcommand::Comment { content } => {
                if args.filters.is_empty() {
                    no_filter_warn()
                }
                for task in tasks {
                    let comment = if content.is_empty() {
                        prompt_text(task.clone(), "comment")?
                    } else {
                        Some(content.join(" "))
                    };

                    if comment.clone().unwrap().trim().is_empty() || comment.is_none() {
                        error!("Abort due to empty comment.");
                        exit(1)
                    }

                    let res = tau.set_comment(task.id.into(), comment.unwrap().trim()).await?;
                    if res {
                        let tsk = tau.get_task_by_id(task.id.into()).await?;
                        print_task_info(tsk)?;
                        println!()
                    }
                }
                Ok(())
            }

            TauSubcommand::Info => {
                for task in tasks {
                    let task = tau.get_task_by_id(task.id.into()).await?;
                    print_task_info(task)?;
                    println!()
                }
                Ok(())
            }

            TauSubcommand::Switch { workspace } => {
                if tau.switch_ws(workspace.clone()).await? {
                    println!("You are now on \"{}\" workspace", workspace);
                }

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
                match month {
                    Some(date) => {
                        let ts =
                            to_naivedate(date.clone())?.and_hms_opt(12, 0, 0).unwrap().timestamp();
                        let tasks = tau.get_stop_tasks(Some(ts)).await?;
                        drawdown(date, tasks, assignee)?;
                    }
                    None => {
                        let ws = tau.get_ws().await?;
                        let tasks = tau.get_stop_tasks(None).await?;
                        print_task_list(tasks, ws)?;
                    }
                }

                Ok(())
            }

            TauSubcommand::List => {
                let ws = tau.get_ws().await?;
                print_task_list(tasks, ws)
            }
        },
        None => {
            let ws = tau.get_ws().await?;
            print_task_list(tasks, ws)
        }
    }?;

    tau.close_connection().await
}
