use std::path::PathBuf;

use chrono::{TimeZone, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    error::{TaudError, TaudResult},
    task_info::TaskInfo,
    util::{get_current_time, Settings, Timestamp},
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MonthTasks {
    created_at: Timestamp,
    settings: Settings,
    task_tks: Vec<String>,
}

impl MonthTasks {
    pub fn new(task_tks: &[String], settings: &Settings) -> Self {
        Self {
            created_at: get_current_time(),
            settings: settings.clone(),
            task_tks: task_tks.to_owned(),
        }
    }

    pub fn add(&mut self, ref_id: &str) {
        self.task_tks.push(ref_id.into());
    }

    pub fn objects(&self) -> TaudResult<Vec<TaskInfo>> {
        let mut tks: Vec<TaskInfo> = vec![];

        for ref_id in self.task_tks.iter() {
            tks.push(TaskInfo::load(ref_id, &self.settings)?);
        }

        Ok(tks)
    }

    pub fn remove(&mut self, ref_id: &str) {
        if let Some(index) = self.task_tks.iter().position(|t| *t == ref_id) {
            self.task_tks.remove(index);
        }
    }

    pub fn set_settings(&mut self, settings: &Settings) {
        self.settings = settings.clone();
    }

    pub fn set_date(&mut self, date: &Timestamp) {
        self.created_at = date.clone();
    }

    pub fn get_task_tks(&self) -> Vec<String> {
        self.task_tks.clone()
    }

    fn get_path(date: &Timestamp, settings: &Settings) -> PathBuf {
        settings
            .dataset_path
            .join("month")
            .join(Utc.timestamp(date.0, 0).format("%m%y").to_string())
    }

    pub fn save(&self) -> TaudResult<()> {
        crate::util::save::<Self>(&Self::get_path(&self.created_at, &self.settings), self)
            .map_err(TaudError::Darkfi)
    }

    pub fn load_or_create(date: &Timestamp, settings: &Settings) -> TaudResult<Self> {
        match crate::util::load::<Self>(&Self::get_path(date, settings)) {
            Ok(mut mt) => {
                mt.set_settings(settings);
                Ok(mt)
            }
            Err(_) => {
                let mut mt = Self::new(&[], settings);
                mt.set_date(date);
                mt.save()?;
                Ok(mt)
            }
        }
    }

    pub fn load_current_open_tasks(settings: &Settings) -> TaudResult<Vec<TaskInfo>> {
        let mt = Self::load_or_create(&get_current_time(), settings)?;
        Ok(mt.objects()?.into_iter().filter(|t| t.get_state() != "stop").collect())
    }
}
