use std::{fmt, str::FromStr};

use darkfi::{util::Timestamp, Error, Result};

use crate::due_as_timestamp;

pub enum State {
    Open,
    Start,
    Pause,
    Stop,
}

impl State {
    pub const fn is_start(&self) -> bool {
        matches!(*self, Self::Start)
    }
    pub const fn is_pause(&self) -> bool {
        matches!(*self, Self::Pause)
    }
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            State::Open => write!(f, "open"),
            State::Start => write!(f, "start"),
            State::Stop => write!(f, "stop"),
            State::Pause => write!(f, "pause"),
        }
    }
}

impl FromStr for State {
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let result = match s.to_lowercase().as_str() {
            "open" => State::Open,
            "stop" => State::Stop,
            "start" => State::Start,
            "pause" => State::Pause,
            _ => return Err(Error::ParseFailed("unable to parse state")),
        };
        Ok(result)
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct BaseTask {
    pub title: String,
    pub desc: Option<String>,
    pub assign: Vec<String>,
    pub project: Vec<String>,
    pub due: Option<i64>,
    pub rank: Option<f32>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct TaskInfo {
    pub ref_id: String,
    pub workspace: String,
    pub id: u32,
    pub title: String,
    pub desc: String,
    pub owner: String,
    pub assign: Vec<String>,
    pub project: Vec<String>,
    pub due: Option<i64>,
    pub rank: Option<f32>,
    pub created_at: i64,
    pub state: String,
    pub events: Vec<TaskEvent>,
    pub comments: Vec<Comment>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct TaskEvent {
    pub action: String,
    pub author: String,
    pub content: String,
    pub timestamp: Timestamp,
}

impl std::fmt::Display for TaskEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "action: {}, timestamp: {}", self.action, self.timestamp)
    }
}

impl Default for TaskEvent {
    fn default() -> Self {
        Self {
            action: State::Open.to_string(),
            author: "".to_string(),
            content: "".to_string(),
            timestamp: Timestamp::current_time(),
        }
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
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
            title.push_str(field[0]);
            title.push(' ');
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
    let title = title.trim().into();
    Ok(BaseTask { title, desc, project, assign, due, rank })
}
