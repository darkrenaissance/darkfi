use std::{
    io,
    path::{Path, PathBuf},
};

use log::debug;
use serde::{Deserialize, Serialize};

use darkfi::util::{
    file::{load_json_file, save_json_file},
    gen_id,
    serial::{Decodable, Encodable, SerialDecodable, SerialEncodable, VarInt},
    Timestamp,
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskEvents(Vec<TaskEvent>);
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskComments(Vec<Comment>);
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskProjects(Vec<String>);
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskAssigns(Vec<String>);

#[derive(Clone, Debug, Serialize, Deserialize, SerialEncodable, SerialDecodable, PartialEq)]
pub struct TaskInfo {
    pub(crate) ref_id: String,
    pub(crate) workspace: String,
    id: u32,
    title: String,
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
    }

    pub fn set_desc(&mut self, desc: &str) {
        debug!(target: "tau", "TaskInfo::set_desc()");
        self.desc = desc.into();
    }

    pub fn set_assign(&mut self, assign: &[String]) {
        debug!(target: "tau", "TaskInfo::set_assign()");
        self.assign = TaskAssigns(assign.to_owned());
    }

    pub fn set_project(&mut self, project: &[String]) {
        debug!(target: "tau", "TaskInfo::set_project()");
        self.project = TaskProjects(project.to_owned());
    }

    pub fn set_comment(&mut self, c: Comment) {
        debug!(target: "tau", "TaskInfo::set_comment()");
        self.comments.0.push(c.clone());
    }

    pub fn set_rank(&mut self, r: Option<f32>) {
        debug!(target: "tau", "TaskInfo::set_rank()");
        self.rank = r;
    }

    pub fn set_due(&mut self, d: Option<Timestamp>) {
        debug!(target: "tau", "TaskInfo::set_due()");
        self.due = d;
    }

    pub fn set_event(&mut self, action: &str, owner: &str, content: &str) {
        debug!(target: "tau", "TaskInfo::set_event()");
        if !content.is_empty() {
            self.events.0.push(TaskEvent::new(action.into(), owner.into(), content.into()));
        }
    }

    pub fn set_state(&mut self, state: &str) {
        debug!(target: "tau", "TaskInfo::set_state()");
        if self.get_state() == state {
            return
        }
        self.state = state.to_string();
    }
}

impl Encodable for TaskEvents {
    fn encode<S: io::Write>(&self, s: S) -> darkfi::Result<usize> {
        encode_vec(&self.0, s)
    }
}

impl Decodable for TaskEvents {
    fn decode<D: io::Read>(d: D) -> darkfi::Result<Self> {
        Ok(Self(decode_vec(d)?))
    }
}
impl Encodable for TaskComments {
    fn encode<S: io::Write>(&self, s: S) -> darkfi::Result<usize> {
        encode_vec(&self.0, s)
    }
}

impl Decodable for TaskComments {
    fn decode<D: io::Read>(d: D) -> darkfi::Result<Self> {
        Ok(Self(decode_vec(d)?))
    }
}
impl Encodable for TaskProjects {
    fn encode<S: io::Write>(&self, s: S) -> darkfi::Result<usize> {
        encode_vec(&self.0, s)
    }
}

impl Decodable for TaskProjects {
    fn decode<D: io::Read>(d: D) -> darkfi::Result<Self> {
        Ok(Self(decode_vec(d)?))
    }
}

impl Encodable for TaskAssigns {
    fn encode<S: io::Write>(&self, s: S) -> darkfi::Result<usize> {
        encode_vec(&self.0, s)
    }
}

impl Decodable for TaskAssigns {
    fn decode<D: io::Read>(d: D) -> darkfi::Result<Self> {
        Ok(Self(decode_vec(d)?))
    }
}

fn encode_vec<T: Encodable, S: io::Write>(vec: &[T], mut s: S) -> darkfi::Result<usize> {
    let mut len = 0;
    len += VarInt(vec.len() as u64).encode(&mut s)?;
    for c in vec.iter() {
        len += c.encode(&mut s)?;
    }
    Ok(len)
}

fn decode_vec<T: Decodable, D: io::Read>(mut d: D) -> darkfi::Result<Vec<T>> {
    let len = VarInt::decode(&mut d)?.0;
    let mut ret = Vec::with_capacity(len as usize);
    for _ in 0..len {
        ret.push(Decodable::decode(&mut d)?);
    }
    Ok(ret)
}
