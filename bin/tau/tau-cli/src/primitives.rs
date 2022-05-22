use darkfi::{util::Timestamp, Result};

use crate::due_as_timestamp;

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct BaseTask {
    pub title: String,
    pub desc: Option<String>,
    pub assign: Vec<String>,
    pub project: Vec<String>,
    pub due: Option<i64>,
    pub rank: Option<f32>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct TaskInfo {
    pub ref_id: String,
    pub id: u32,
    pub title: String,
    pub desc: String,
    pub owner: String,
    pub assign: Vec<String>,
    pub project: Vec<String>,
    pub due: Option<i64>,
    pub rank: f32,
    pub created_at: i64,
    pub events: Vec<TaskEvent>,
    pub comments: Vec<Comment>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct TaskEvent {
    pub action: String,
    pub timestamp: Timestamp,
}

impl std::fmt::Display for TaskEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "action: {}, timestamp: {}", self.action, self.timestamp)
    }
}

impl Default for TaskEvent {
    fn default() -> Self {
        Self { action: "open".into(), timestamp: Timestamp::current_time() }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct Comment {
    content: String,
    author: String,
    timestamp: Timestamp,
}

impl std::fmt::Display for Comment {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{} author: {}, content: {} ", self.timestamp, self.author, self.content)
    }
}

pub fn task_from_cli(values: Vec<String>) -> Result<BaseTask> {
    let mut title = String::new();
    let mut desc = None;
    let mut project = vec![];
    let mut assign = vec![];
    let mut due = None;
    let mut rank = None;

    for val in values {
        let field: Vec<&str> = val.split(':').collect();
        if field.len() == 1 {
            title = field[0].into();
            continue
        }

        if field.len() != 2 {
            continue
        }

        if field[0] == "project" {
            project = field[1].split(',').map(|s| s.into()).collect();
        }

        if field[0] == "desc" {
            desc = Some(field[1].into());
        }

        if field[0] == "assign" {
            assign = field[1].split(',').map(|s| s.into()).collect();
        }

        if field[0] == "due" {
            due = due_as_timestamp(field[1])
        }

        if field[0] == "rank" {
            rank = Some(field[1].parse::<f32>()?);
        }
    }

    Ok(BaseTask { title, desc, project, assign, due, rank })
}
