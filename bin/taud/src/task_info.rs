use chrono::Utc;
use rand::Rng;
use serde::{Deserialize, Serialize};

use darkfi::Result;

use crate::util::{random_ref_id, Settings, Timestamp};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct TaskEvent {
    action: String,
    timestamp: Timestamp,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct Comment {
    content: String,
    author: String,
    timestamp: Timestamp,
}

// XXX
#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TaskInfo {
    ref_id: String,
    id: u32,
    pub title: String,
    desc: String,
    assign: Vec<String>,
    project: Vec<String>,
    due: Option<Timestamp>,
    rank: u32,
    created_at: Timestamp,
    events: Vec<TaskEvent>,
    comments: Vec<Comment>,
}

impl TaskInfo {
    pub fn new(title: &str, desc: &str, due: Option<Timestamp>, rank: u32) -> Self {
        // TODO
        // check due date

        // generate ref_id
        let ref_id = random_ref_id();

        // XXX must find the next free id
        let mut rng = rand::thread_rng();
        let id: u32 = rng.gen();

        let created_at: Timestamp = Timestamp(Utc::now().to_string());

        Self {
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
        }
    }

    pub fn assign(&mut self, n: String) {
        self.assign.push(n);
    }

    pub fn project(&mut self, p: String) {
        self.project.push(p);
    }

    pub fn load(_tk_hash: &str, _settings: &Settings) -> Result<Self> {
        Ok(Self::new("test", "test", None, 0))
    }

    pub fn save(&self, _settings: &Settings) -> Result<()> {
        Ok(())
    }
}
