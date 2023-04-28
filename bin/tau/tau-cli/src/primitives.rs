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

use std::{fmt, str::FromStr};

use darkfi::{util::time::NanoTimestamp, Error, Result};

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
    pub const fn is_stop(&self) -> bool {
        matches!(*self, Self::Stop)
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

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BaseTask {
    pub title: String,
    pub tags: Vec<String>,
    pub desc: Option<String>,
    pub assign: Vec<String>,
    pub project: Vec<String>,
    pub due: Option<i64>,
    pub rank: Option<f32>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct TaskInfo {
    pub ref_id: String,
    pub workspace: String,
    pub id: u32,
    pub title: String,
    pub tags: Vec<String>,
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

impl From<BaseTask> for TaskInfo {
    fn from(value: BaseTask) -> Self {
        Self {
            ref_id: String::default(),
            workspace: String::default(),
            id: u32::default(),
            title: value.title,
            tags: value.tags,
            desc: String::default(),
            owner: String::default(),
            assign: value.assign,
            project: value.project,
            due: value.due,
            rank: value.rank,
            created_at: i64::default(),
            state: String::default(),
            events: vec![],
            comments: vec![],
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct TaskEvent {
    pub action: String,
    pub author: String,
    pub content: String,
    pub timestamp: NanoTimestamp,
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
            timestamp: NanoTimestamp::current_time(),
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Comment {
    content: String,
    author: String,
    timestamp: NanoTimestamp,
}

impl std::fmt::Display for Comment {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{} author: {}, content: {} ", self.timestamp, self.author, self.content)
    }
}

pub fn task_from_cli(values: Vec<String>) -> Result<BaseTask> {
    let mut title = String::new();
    let mut tags = vec![];
    let mut desc = None;
    let mut project = vec![];
    let mut assign = vec![];
    let mut due = None;
    let mut rank = None;

    for val in values {
        let field: Vec<&str> = val.split(':').collect();
        if field.len() == 1 {
            if field[0].starts_with('+') || field[0].starts_with('-') {
                tags.push(field[0].into());
                continue
            }
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
    Ok(BaseTask { title, tags, desc, project, assign, due, rank })
}
