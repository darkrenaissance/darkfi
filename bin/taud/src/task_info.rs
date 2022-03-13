use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use darkfi::Result;

use crate::{
    month_tasks::MonthTasks,
    util::{find_free_id, get_current_time, random_ref_id, Settings, Timestamp},
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]

struct TaskEvent {
    action: String,
    timestamp: Timestamp,
}

impl TaskEvent {
    fn new(action: String) -> Self {
        Self { action, timestamp: get_current_time() }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Comment {
    content: String,
    author: String,
    timestamp: Timestamp,
}

impl Comment {
    pub fn new(content: &str, author: &str) -> Self {
        Self { content: content.into(), author: author.into(), timestamp: get_current_time() }
    }
}

// XXX
#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TaskInfo {
    ref_id: String,
    id: u32,
    title: String,
    desc: String,
    assign: Vec<String>,
    project: Vec<String>,
    due: Option<Timestamp>,
    rank: u32,
    created_at: Timestamp,
    events: Vec<TaskEvent>,
    comments: Vec<Comment>,
    #[serde(skip_serializing, skip_deserializing)]
    settings: Settings,
}

impl TaskInfo {
    pub fn new(
        title: &str,
        desc: &str,
        due: Option<Timestamp>,
        rank: u32,
        settings: &Settings,
    ) -> Result<Self> {
        // generate ref_id
        let ref_id = random_ref_id();

        let created_at: Timestamp = get_current_time();

        let task_ids: Vec<u32> =
            MonthTasks::load_current_open_tasks(&settings)?.into_iter().map(|t| t.id).collect();
        let id: u32 = find_free_id(&task_ids);

        Ok(Self {
            ref_id,
            id,
            title: title.into(),
            desc: desc.into(),
            assign: vec![],
            project: vec![],
            due,
            rank,
            created_at,
            comments: vec![],
            events: vec![],
            settings: settings.clone(),
        })
    }

    pub fn load(ref_id: &str, settings: &Settings) -> Result<Self> {
        let mut task = crate::util::load::<Self>(&Self::get_path(ref_id, settings))?;
        task.set_settings(settings);
        Ok(task)
    }

    pub fn save(&self) -> Result<()> {
        crate::util::save::<Self>(&Self::get_path(&self.ref_id, &self.settings), self)
    }

    pub fn activate(&self) -> Result<()> {
        let mut mt = MonthTasks::load_or_create(&self.created_at, &self.settings)?;
        mt.add(&self.ref_id);
        mt.save()
    }

    pub fn get_state(&self) -> String {
        if let Some(ev) = self.events.last() {
            return ev.action.clone()
        } else {
            return "open".into()
        }
    }

    fn get_path(ref_id: &str, settings: &Settings) -> PathBuf {
        settings.dataset_path.join("task").join(ref_id)
    }

    pub fn get_id(&self) -> u32 {
        self.id.clone()
    }

    pub fn get_ref_id(&self) -> String {
        self.ref_id.clone()
    }

    pub fn set_title(&mut self, title: &str) {
        self.title = title.into();
    }

    pub fn set_desc(&mut self, desc: &str) {
        self.desc = desc.into();
    }

    pub fn set_assign(&mut self, assign: &Vec<String>) {
        self.assign = assign.clone();
    }

    pub fn set_project(&mut self, project: &Vec<String>) {
        self.project = project.clone();
    }

    pub fn set_comment(&mut self, c: Comment) {
        self.comments.push(c);
    }

    pub fn set_rank(&mut self, r: u32) {
        self.rank = r;
    }

    pub fn set_due(&mut self, d: Option<Timestamp>) {
        self.due = d;
    }

    pub fn set_settings(&mut self, settings: &Settings) {
        self.settings = settings.clone();
    }

    pub fn set_state(&mut self, action: &str) {
        if self.get_state() == action {
            return
        }
        self.events.push(TaskEvent::new(action.into()));
    }
}
