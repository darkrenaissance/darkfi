use chrono::Utc;
use serde::{Deserialize, Serialize};

use darkfi::Result;

use crate::{
    task_info::TaskInfo,
    util::{Settings, Timestamp},
};

// XXX
#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MonthTasks {
    pub created_at: Timestamp,
    #[serde(skip_serializing, skip_deserializing)]
    pub settings: Settings,
    pub task_tks: Vec<String>,
}

impl MonthTasks {
    pub fn add(&mut self, tk_hash: &str) {
        self.task_tks.push(tk_hash.into());
    }

    pub fn objects(&self) -> Result<Vec<TaskInfo>> {
        let mut tks: Vec<TaskInfo> = vec![];

        for tk_hash in self.task_tks.iter() {
            tks.push(TaskInfo::load(&tk_hash, &self.settings)?);
        }

        Ok(tks)
    }

    pub fn remove(&mut self, tk_hash: &str) {
        if let Some(index) = self.task_tks.iter().position(|t| *t == tk_hash) {
            self.task_tks.remove(index);
        }
    }

    fn load(_date: Timestamp, _settings: Settings) -> Result<Timestamp> {
        Ok(Timestamp(Utc::now().to_string()))
    }

    fn load_or_create(_date: Timestamp, _settings: Settings) -> Result<Timestamp> {
        Ok(Timestamp(Utc::now().to_string()))
    }
}

impl PartialEq for MonthTasks {
    fn eq(&self, other: &Self) -> bool {
        self.created_at == other.created_at && self.task_tks == other.task_tks
    }
}
