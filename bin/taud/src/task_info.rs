use std::{io, path::PathBuf};

use serde::{Deserialize, Serialize};

use darkfi::{
    util::serial::{Decodable, Encodable},
    Result,
};

use darkfi::util::serial::VarInt;

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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct TaskEvents(Vec<TaskEvent>);
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct TaskComments(Vec<Comment>);
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct TaskProjects(Vec<String>);
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct TaskAssigns(Vec<String>);

// XXX
#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TaskInfo {
    ref_id: String,
    id: u32,
    title: String,
    desc: String,
    assign: TaskAssigns,
    project: TaskProjects,
    due: Option<Timestamp>,
    rank: u32,
    created_at: Timestamp,
    events: TaskEvents,
    comments: TaskComments,
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
            MonthTasks::load_current_open_tasks(settings)?.into_iter().map(|t| t.id).collect();
        let id: u32 = find_free_id(&task_ids);

        Ok(Self {
            ref_id,
            id,
            title: title.into(),
            desc: desc.into(),
            assign: TaskAssigns(vec![]),
            project: TaskProjects(vec![]),
            due,
            rank,
            created_at,
            comments: TaskComments(vec![]),
            events: TaskEvents(vec![]),
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
        if let Some(ev) = self.events.0.last() {
            ev.action.clone()
        } else {
            "open".into()
        }
    }

    fn get_path(ref_id: &str, settings: &Settings) -> PathBuf {
        settings.dataset_path.join("task").join(ref_id)
    }

    pub fn get_id(&self) -> u32 {
        self.id
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
        self.assign = TaskAssigns(assign.clone());
    }

    pub fn set_project(&mut self, project: &Vec<String>) {
        self.project = TaskProjects(project.clone());
    }

    pub fn set_comment(&mut self, c: Comment) {
        self.comments.0.push(c);
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
        self.events.0.push(TaskEvent::new(action.into()));
    }
}

impl Encodable for TaskEvent {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.action.encode(&mut s)?;
        len += self.timestamp.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for TaskEvent {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self { action: Decodable::decode(&mut d)?, timestamp: Decodable::decode(&mut d)? })
    }
}

impl Encodable for Comment {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.content.encode(&mut s)?;
        len += self.author.encode(&mut s)?;
        len += self.timestamp.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for Comment {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            content: Decodable::decode(&mut d)?,
            author: Decodable::decode(&mut d)?,
            timestamp: Decodable::decode(&mut d)?,
        })
    }
}

impl Encodable for TaskInfo {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.ref_id.encode(&mut s)?;
        len += self.id.encode(&mut s)?;
        len += self.title.encode(&mut s)?;
        len += self.desc.encode(&mut s)?;
        len += self.assign.encode(&mut s)?;
        len += self.project.encode(&mut s)?;
        len += self.due.encode(&mut s)?;
        len += self.rank.encode(&mut s)?;
        len += self.created_at.encode(&mut s)?;
        len += self.events.encode(&mut s)?;
        len += self.comments.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for TaskInfo {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            ref_id: Decodable::decode(&mut d)?,
            id: Decodable::decode(&mut d)?,
            title: Decodable::decode(&mut d)?,
            desc: Decodable::decode(&mut d)?,
            assign: Decodable::decode(&mut d)?,
            project: Decodable::decode(&mut d)?,
            due: Decodable::decode(&mut d)?,
            rank: Decodable::decode(&mut d)?,
            created_at: Decodable::decode(&mut d)?,
            events: Decodable::decode(&mut d)?,
            comments: Decodable::decode(&mut d)?,
            settings: Settings::default(),
        })
    }
}

impl Encodable for TaskEvents {
    fn encode<S: io::Write>(&self, s: S) -> Result<usize> {
        encode_vec(&self.0, s)
    }
}

impl Decodable for TaskEvents {
    fn decode<D: io::Read>(d: D) -> Result<Self> {
        Ok(Self(decode_vec(d)?))
    }
}
impl Encodable for TaskComments {
    fn encode<S: io::Write>(&self, s: S) -> Result<usize> {
        encode_vec(&self.0, s)
    }
}

impl Decodable for TaskComments {
    fn decode<D: io::Read>(d: D) -> Result<Self> {
        Ok(Self(decode_vec(d)?))
    }
}
impl Encodable for TaskProjects {
    fn encode<S: io::Write>(&self, s: S) -> Result<usize> {
        encode_vec(&self.0, s)
    }
}

impl Decodable for TaskProjects {
    fn decode<D: io::Read>(d: D) -> Result<Self> {
        Ok(Self(decode_vec(d)?))
    }
}

impl Encodable for TaskAssigns {
    fn encode<S: io::Write>(&self, s: S) -> Result<usize> {
        encode_vec(&self.0, s)
    }
}

impl Decodable for TaskAssigns {
    fn decode<D: io::Read>(d: D) -> Result<Self> {
        Ok(Self(decode_vec(d)?))
    }
}

fn encode_vec<T: Encodable, S: io::Write>(vec: &Vec<T>, mut s: S) -> Result<usize> {
    let mut len = 0;
    len += VarInt(vec.len() as u64).encode(&mut s)?;
    for c in vec.iter() {
        len += c.encode(&mut s)?;
    }
    Ok(len)
}

fn decode_vec<T: Decodable, D: io::Read>(mut d: D) -> Result<Vec<T>> {
    let len = VarInt::decode(&mut d)?.0;
    let mut ret = Vec::with_capacity(len as usize);
    for _ in 0..len {
        ret.push(Decodable::decode(&mut d)?);
    }
    Ok(ret)
}
