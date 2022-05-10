use serde::{Deserialize, Serialize};
use structopt::StructOpt;
use structopt_toml::StructOptToml;

use darkfi::Result;

use super::util::due_as_timestamp;

/// Tau cli
#[derive(Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "tau")]
pub struct CliTau {
    /// Increase verbosity
    #[structopt(short, parse(from_occurrences))]
    pub verbose: u8,
    /// JSON-RPC listen URL
    #[structopt(long = "rpc", default_value = "tcp://127.0.0.1:11055")]
    pub rpc_listen: String,
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

#[derive(StructOpt, Deserialize, Debug)]
pub enum CliTauSubCommands {
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
    List {},
}

#[derive(Serialize, Debug, Clone)]
pub struct CliBaseTask {
    pub title: String,
    pub desc: Option<String>,
    pub assign: Vec<String>,
    pub project: Vec<String>,
    pub due: Option<i64>,
    pub rank: Option<f32>,
}

pub fn task_from_cli_values(values: Vec<String>) -> Result<CliBaseTask> {
    let mut title: String = String::new();
    let mut desc: Option<String> = None;
    let mut project: Vec<String> = vec![];
    let mut assign: Vec<String> = vec![];
    let mut due: Option<i64> = None;
    let mut rank: Option<f32> = None;

    for val in values {
        let field: Vec<&str> = val.split(':').collect();
        if field.len() == 1 {
            title = field[0].into();
            continue
        }

        if field.len() != 2 {
            continue
        }

        if field[0].starts_with("project") {
            project.push(field[1].into());
        }
        if field[0].starts_with("desc") {
            desc = Some(field[1].into());
        }
        if field[0].starts_with("assign") {
            assign.push(field[1].into());
        }
        if field[0].starts_with("due") {
            due = due_as_timestamp(&field[1])
        }
        if field[0].starts_with("rank") {
            rank = Some(field[1].parse::<f32>()?);
        }
    }

    let task = CliBaseTask { title, desc, project, assign, due, rank };

    Ok(task)
}
