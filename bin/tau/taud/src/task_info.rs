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

use std::path::{Path, PathBuf};

use darkfi_serial::{SerialDecodable, SerialEncodable};
use log::debug;
use tinyjson::JsonValue;

use darkfi::{
    event_graph::gen_id,
    util::{
        file::{load_json_file, save_json_file},
        time::Timestamp,
    },
};

use crate::{
    error::{TaudError, TaudResult},
    month_tasks::MonthTasks,
    util::find_free_id,
};

#[derive(Clone, Debug, SerialEncodable, SerialDecodable, PartialEq, Eq)]
pub struct TaskEvent {
    pub action: String,
    pub author: String,
    pub content: String,
    pub timestamp: Timestamp,
}

impl TaskEvent {
    pub fn new(action: String, author: String, content: String) -> Self {
        Self { action, author, content, timestamp: Timestamp::current_time() }
    }
}

impl From<TaskEvent> for JsonValue {
    fn from(task_event: TaskEvent) -> JsonValue {
        JsonValue::Object(HashMap::from([
            ("action".to_string(), JsonValue::String(task_event.action.clone())),
            ("author".to_string(), JsonValue::String(task_event.author.clone())),
            ("content".to_string(), JsonValue::String(task_event.content.clone())),
            ("timestamp".to_string(), JsonValue::String(task_event.timestamp.0.to_string())),
        ]))
    }
}

impl From<&JsonValue> for TaskEvent {
    fn from(value: &JsonValue) -> TaskEvent {
        let map = value.get::<HashMap<String, JsonValue>>().unwrap();
        TaskEvent {
            action: map["action"].get().unwrap().clone(),
            author: map["author"].get().unwrap().clone(),
            content: map["content"].get().unwrap().clone(),
            timestamp: Timestamp(
                u64::from_str_radix(map["timestamp"].get::<String>().unwrap(), 10).unwrap(),
            ),
        }
    }
}

#[derive(Clone, Debug, SerialDecodable, SerialEncodable, PartialEq, Eq)]
pub struct Comment {
    content: String,
    author: String,
    timestamp: Timestamp,
}

impl From<Comment> for JsonValue {
    fn from(comment: Comment) -> JsonValue {
        JsonValue::Object(HashMap::from([
            ("content".to_string(), JsonValue::String(comment.content.clone())),
            ("author".to_string(), JsonValue::String(comment.author.clone())),
            ("timestamp".to_string(), JsonValue::String(comment.timestamp.0.to_string())),
        ]))
    }
}

impl From<JsonValue> for Comment {
    fn from(value: JsonValue) -> Comment {
        let map = value.get::<HashMap<String, JsonValue>>().unwrap();
        Comment {
            content: map["content"].get().unwrap().clone(),
            author: map["author"].get().unwrap().clone(),
            timestamp: Timestamp(
                u64::from_str_radix(map["timestamp"].get::<String>().unwrap(), 10).unwrap(),
            ),
        }
    }
}

impl Comment {
    pub fn new(content: &str, author: &str) -> Self {
        Self {
            content: content.into(),
            author: author.into(),
            timestamp: Timestamp::current_time(),
        }
    }
}

#[derive(Clone, Debug, SerialEncodable, SerialDecodable, PartialEq)]
pub struct TaskInfo {
    pub(crate) ref_id: String,
    pub(crate) workspace: String,
    pub(crate) id: u32,
    pub(crate) title: String,
    tags: Vec<String>,
    desc: String,
    pub(crate) owner: String,
    assign: Vec<String>,
    project: Vec<String>,
    due: Option<Timestamp>,
    rank: Option<f32>,
    created_at: Timestamp,
    state: String,
    pub(crate) events: Vec<TaskEvent>,
    comments: Vec<Comment>,
}

impl From<&TaskInfo> for JsonValue {
    fn from(task: &TaskInfo) -> JsonValue {
        let ref_id = JsonValue::String(task.ref_id.clone());
        let workspace = JsonValue::String(task.workspace.clone());
        let id = JsonValue::Number(task.id.into());
        let title = JsonValue::String(task.title.clone());
        let tags: Vec<JsonValue> = task.tags.iter().map(|x| JsonValue::String(x.clone())).collect();
        let desc = JsonValue::String(task.desc.clone());
        let owner = JsonValue::String(task.owner.clone());

        let assign: Vec<JsonValue> =
            task.assign.iter().map(|x| JsonValue::String(x.clone())).collect();

        let project: Vec<JsonValue> =
            task.project.iter().map(|x| JsonValue::String(x.clone())).collect();

        let due = if let Some(ts) = task.due {
            JsonValue::String(ts.0.to_string())
        } else {
            JsonValue::Null
        };

        let rank = if let Some(rank) = task.rank {
            JsonValue::Number(rank.into())
        } else {
            JsonValue::Null
        };

        let created_at = JsonValue::String(task.created_at.0.to_string());
        let state = JsonValue::String(task.state.clone());
        let events: Vec<JsonValue> = task.events.iter().map(|x| x.clone().into()).collect();
        let comments: Vec<JsonValue> = task.comments.iter().map(|x| x.clone().into()).collect();

        JsonValue::Object(HashMap::from([
            ("ref_id".to_string(), ref_id),
            ("workspace".to_string(), workspace),
            ("id".to_string(), id),
            ("title".to_string(), title),
            ("tags".to_string(), JsonValue::Array(tags)),
            ("desc".to_string(), desc),
            ("owner".to_string(), owner),
            ("assign".to_string(), JsonValue::Array(assign)),
            ("project".to_string(), JsonValue::Array(project)),
            ("due".to_string(), due),
            ("rank".to_string(), rank),
            ("created_at".to_string(), created_at),
            ("state".to_string(), state),
            ("events".to_string(), JsonValue::Array(events)),
            ("comments".to_string(), JsonValue::Array(comments)),
        ]))
    }
}

impl From<JsonValue> for TaskInfo {
    fn from(value: JsonValue) -> TaskInfo {
        let tags = value["tags"].get::<Vec<JsonValue>>().unwrap();
        let assign = value["assign"].get::<Vec<JsonValue>>().unwrap();
        let project = value["project"].get::<Vec<JsonValue>>().unwrap();
        let events = value["events"].get::<Vec<JsonValue>>().unwrap();
        let comments = value["comments"].get::<Vec<JsonValue>>().unwrap();

        let due = {
            if value["due"].is_null() {
                None
            } else {
                let u64_str = value["due"].get::<String>().unwrap();
                Some(Timestamp(u64::from_str_radix(u64_str, 10).unwrap()))
            }
        };

        let rank = {
            if value["rank"].is_null() {
                None
            } else {
                Some(*value["rank"].get::<f64>().unwrap() as f32)
            }
        };

        let created_at = {
            let u64_str = value["created_at"].get::<String>().unwrap();
            Timestamp(u64::from_str_radix(u64_str, 10).unwrap())
        };

        let events: Vec<TaskEvent> = events.iter().map(|x| x.into()).collect();
        let comments: Vec<Comment> = comments.iter().map(|x| (*x).into()).collect();

        TaskInfo {
            ref_id: value["ref_id"].get::<String>().unwrap().clone(),
            workspace: value["workspace"].get::<String>().unwrap().clone(),
            id: *value["id"].get::<f64>().unwrap() as u32,
            title: value["title"].get::<String>().unwrap().clone(),
            tags: tags.iter().map(|x| x.get::<String>().unwrap().clone()).collect(),
            desc: value["desc"].get::<String>().unwrap().clone(),
            owner: value["owner"].get::<String>().unwrap().clone(),
            assign: assign.iter().map(|x| x.get::<String>().unwrap().clone()).collect(),
            project: project.iter().map(|x| x.get::<String>().unwrap().clone()).collect(),
            due,
            rank,
            created_at,
            state: value["state"].get::<String>().unwrap().clone(),
            events,
            comments,
        }
    }
}

impl TaskInfo {
    pub fn new(
        workspace: String,
        title: &str,
        desc: &str,
        owner: &str,
        due: Option<Timestamp>,
        rank: Option<f32>,
        dataset_path: &Path,
    ) -> TaudResult<Self> {
        // generate ref_id
        let ref_id = gen_id(30);

        let created_at = Timestamp::current_time();

        let task_ids: Vec<u32> =
            MonthTasks::load_current_tasks(dataset_path, workspace.clone(), false)?
                .into_iter()
                .map(|t| t.id)
                .collect();

        let id: u32 = find_free_id(&task_ids);

        if let Some(d) = &due {
            if *d < Timestamp::current_time() {
                return Err(TaudError::InvalidDueTime)
            }
        }

        Ok(Self {
            ref_id,
            workspace,
            id,
            title: title.into(),
            desc: desc.into(),
            owner: owner.into(),
            tags: vec![],
            assign: vec![],
            project: vec![],
            due,
            rank,
            created_at,
            state: "open".into(),
            comments: vec![],
            events: vec![],
        })
    }

    pub fn load(ref_id: &str, dataset_path: &Path) -> TaudResult<Self> {
        debug!(target: "tau", "TaskInfo::load()");
        let task = load_json_file(&Self::get_path(ref_id, dataset_path))?;
        Ok(task.into())
    }

    pub fn save(&self, dataset_path: &Path) -> TaudResult<()> {
        debug!(target: "tau", "TaskInfo::save()");
        save_json_file(&Self::get_path(&self.ref_id, dataset_path), &self.into(), true)
            .map_err(TaudError::Darkfi)?;

        if self.get_state() == "stop" {
            self.deactivate(dataset_path)?;
        } else {
            self.activate(dataset_path)?;
        }

        Ok(())
    }

    pub fn activate(&self, path: &Path) -> TaudResult<()> {
        debug!(target: "tau", "TaskInfo::activate()");
        let mut mt = MonthTasks::load_or_create(Some(&self.created_at), path)?;
        mt.add(&self.ref_id);
        mt.save(path)
    }

    pub fn deactivate(&self, path: &Path) -> TaudResult<()> {
        debug!(target: "tau", "TaskInfo::deactivate()");
        let mut mt = MonthTasks::load_or_create(Some(&self.created_at), path)?;
        mt.remove(&self.ref_id);
        mt.save(path)
    }

    pub fn get_state(&self) -> String {
        debug!(target: "tau", "TaskInfo::get_state()");
        self.state.clone()
    }

    pub fn get_path(ref_id: &str, dataset_path: &Path) -> PathBuf {
        debug!(target: "tau", "TaskInfo::get_path()");
        dataset_path.join("task").join(ref_id)
    }

    pub fn get_id(&self) -> u32 {
        debug!(target: "tau", "TaskInfo::get_id()");
        self.id
    }

    pub fn set_title(&mut self, title: &str) {
        debug!(target: "tau", "TaskInfo::set_title()");
        self.title = title.into();
    }

    pub fn set_desc(&mut self, desc: &str) {
        debug!(target: "tau", "TaskInfo::set_desc()");
        self.desc = desc.into();
    }

    pub fn set_tags(&mut self, tags: &[String]) {
        debug!(target: "tau", "TaskInfo::set_tags()");
        for tag in tags.iter() {
            if tag.starts_with('+') && !self.tags.contains(tag) {
                self.tags.push(tag.to_string());
            }
            if tag.starts_with('-') {
                let t = tag.replace('-', "+");
                self.tags.retain(|tag| tag != &t);
            }
        }
    }

    pub fn set_assign(&mut self, assigns: &[String]) {
        debug!(target: "tau", "TaskInfo::set_assign()");
        self.assign = assigns.to_owned();
    }

    pub fn set_project(&mut self, projects: &[String]) {
        debug!(target: "tau", "TaskInfo::set_project()");
        self.project = projects.to_owned();
    }

    pub fn set_comment(&mut self, c: Comment) {
        debug!(target: "tau", "TaskInfo::set_comment()");
        self.comments.push(c);
    }

    pub fn set_rank(&mut self, r: Option<f32>) {
        debug!(target: "tau", "TaskInfo::set_rank()");
        self.rank = r;
    }

    pub fn set_due(&mut self, d: Option<Timestamp>) {
        debug!(target: "tau", "TaskInfo::set_due()");
        self.due = d;
    }

    pub fn set_state(&mut self, state: &str) {
        debug!(target: "tau", "TaskInfo::set_state()");
        if self.get_state() == state {
            return
        }
        self.state = state.to_string();
    }
}
