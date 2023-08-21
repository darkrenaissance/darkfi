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

use darkfi::{util::time::Timestamp, Result};

use crate::due_as_timestamp;
pub(crate) use taud::task_info::{State, TaskEvent, TaskInfo};

#[derive(Clone, Debug)]
pub struct BaseTask {
    pub title: String,
    pub tags: Vec<String>,
    pub desc: Option<String>,
    pub assign: Vec<String>,
    pub project: Vec<String>,
    pub due: Option<u64>,
    pub rank: Option<f32>,
}

impl From<BaseTask> for TaskInfo {
    fn from(value: BaseTask) -> Self {
        let due = if let Some(vd) = value.due { Some(Timestamp(vd)) } else { None };

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
            due,
            rank: value.rank,
            created_at: Timestamp(u64::default()),
            state: String::default(),
            events: vec![],
            comments: vec![],
        }
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
            if field[0].starts_with('@') {
                assign.push(field[0].into());
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
