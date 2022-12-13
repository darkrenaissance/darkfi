/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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
use serde::{Deserialize, Serialize};

use darkfi::{
    raft::gen_id,
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

#[derive(Clone, Debug, Serialize, Deserialize, SerialEncodable, SerialDecodable, PartialEq, Eq)]
struct TaskEvent {
    action: String,
    author: String,
    content: String,
    timestamp: Timestamp,
}

impl TaskEvent {
    fn new(action: String, author: String, content: String) -> Self {
        Self { action, author, content, timestamp: Timestamp::current_time() }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, SerialDecodable, SerialEncodable, PartialEq, Eq)]
pub struct Comment {
    content: String,
    author: String,
    timestamp: Timestamp,
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct TaskEvents(Vec<TaskEvent>);
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct TaskComments(Vec<Comment>);
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct TaskProjects(Vec<String>);
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct TaskAssigns(Vec<String>);
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct TaskTags(Vec<String>);

#[derive(Clone, Debug, Serialize, Deserialize, SerialEncodable, SerialDecodable, PartialEq)]
pub struct TaskInfo {
    pub(crate) ref_id: String,
    pub(crate) workspace: String,
    pub(crate) id: u32,
    title: String,
    tags: TaskTags,
    desc: String,
    owner: String,
    assign: TaskAssigns,
    project: TaskProjects,
    due: Option<Timestamp>,
    rank: Option<f32>,
    created_at: Timestamp,
    state: String,
    events: TaskEvents,
    comments: TaskComments,
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
            tags: TaskTags(vec![]),
            assign: TaskAssigns(vec![]),
            project: TaskProjects(vec![]),
            due,
            rank,
            created_at,
            state: "open".into(),
            comments: TaskComments(vec![]),
            events: TaskEvents(vec![]),
        })
    }

    pub fn load(ref_id: &str, dataset_path: &Path) -> TaudResult<Self> {
        debug!(target: "tau", "TaskInfo::load()");
        let task = load_json_file::<Self>(&Self::get_path(ref_id, dataset_path))?;
        Ok(task)
    }

    pub fn save(&self, dataset_path: &Path) -> TaudResult<()> {
        debug!(target: "tau", "TaskInfo::save()");
        save_json_file::<Self>(&Self::get_path(&self.ref_id, dataset_path), self)
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
        self.set_event("title", title);
    }

    pub fn set_desc(&mut self, desc: &str) {
        debug!(target: "tau", "TaskInfo::set_desc()");
        self.desc = desc.into();
        self.set_event("desc", desc);
    }

    pub fn set_tags(&mut self, tags: &[String]) {
        debug!(target: "tau", "TaskInfo::set_tags()");
        for tag in tags.iter() {
            if tag.starts_with('+') && !self.tags.0.contains(tag) {
                self.tags.0.push(tag.to_string());
            }
            if tag.starts_with('-') {
                let t = tag.replace('-', "+");
                self.tags.0.retain(|tag| tag != &t);
            }
        }
        self.set_event("tags", &tags.join(", "));
    }

    pub fn set_assign(&mut self, assigns: &[String]) {
        debug!(target: "tau", "TaskInfo::set_assign()");
        self.assign = TaskAssigns(assigns.to_owned());
        self.set_event("assign", &assigns.join(", "));
    }

    pub fn set_project(&mut self, projects: &[String]) {
        debug!(target: "tau", "TaskInfo::set_project()");
        self.project = TaskProjects(projects.to_owned());
        self.set_event("project", &projects.join(", "));
    }

    pub fn set_comment(&mut self, c: Comment) {
        debug!(target: "tau", "TaskInfo::set_comment()");
        self.comments.0.push(c.clone());
        self.set_event("comment", &c.content);
    }

    pub fn set_rank(&mut self, r: Option<f32>) {
        debug!(target: "tau", "TaskInfo::set_rank()");
        self.rank = r;
        match r {
            Some(v) => {
                self.set_event("rank", &v.to_string());
            }
            None => {
                self.set_event("rank", "None");
            }
        }
    }

    pub fn set_due(&mut self, d: Option<Timestamp>) {
        debug!(target: "tau", "TaskInfo::set_due()");
        self.due = d;
        match d {
            Some(v) => {
                self.set_event("due", &v.to_string());
            }
            None => {
                self.set_event("due", "None");
            }
        }
    }

    pub fn set_event(&mut self, action: &str, content: &str) {
        debug!(target: "tau", "TaskInfo::set_event()");
        if !content.is_empty() {
            self.events.0.push(TaskEvent::new(action.into(), self.owner.clone(), content.into()));
        }
    }

    pub fn set_state(&mut self, state: &str) {
        debug!(target: "tau", "TaskInfo::set_state()");
        if self.get_state() == state {
            return
        }
        self.state = state.to_string();
        self.set_event("state", state);
    }
}
