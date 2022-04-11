use std::path::{Path, PathBuf};

use chrono::{TimeZone, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    error::{TaudError, TaudResult},
    task_info::TaskInfo,
    util::{get_current_time, Timestamp},
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MonthTasks {
    created_at: Timestamp,
    dataset_path: PathBuf,
    task_tks: Vec<String>,
}

impl MonthTasks {
    pub fn new(task_tks: &[String], dataset_path: &Path) -> Self {
        Self {
            created_at: get_current_time(),
            dataset_path: dataset_path.to_path_buf(),
            task_tks: task_tks.to_owned(),
        }
    }

    pub fn add(&mut self, ref_id: &str) {
        self.task_tks.push(ref_id.into());
    }

    pub fn objects(&self) -> TaudResult<Vec<TaskInfo>> {
        let mut tks: Vec<TaskInfo> = vec![];

        for ref_id in self.task_tks.iter() {
            tks.push(TaskInfo::load(ref_id, &self.dataset_path)?);
        }

        Ok(tks)
    }

    pub fn remove(&mut self, ref_id: &str) {
        if let Some(index) = self.task_tks.iter().position(|t| *t == ref_id) {
            self.task_tks.remove(index);
        }
    }

    pub fn set_dataset_path(&mut self, dataset_path: &Path) {
        self.dataset_path = dataset_path.to_path_buf();
    }

    pub fn set_date(&mut self, date: &Timestamp) {
        self.created_at = date.clone();
    }

    fn get_path(date: &Timestamp, dataset_path: &Path) -> PathBuf {
        dataset_path.join("month").join(Utc.timestamp(date.0, 0).format("%m%y").to_string())
    }

    pub fn save(&self) -> TaudResult<()> {
        crate::util::save::<Self>(&Self::get_path(&self.created_at, &self.dataset_path), self)
            .map_err(TaudError::Darkfi)
    }

    pub fn load_or_create(date: &Timestamp, dataset_path: &Path) -> TaudResult<Self> {
        match crate::util::load::<Self>(&Self::get_path(date, dataset_path)) {
            Ok(mut mt) => {
                mt.set_dataset_path(dataset_path);
                Ok(mt)
            }
            Err(_) => {
                let mut mt = Self::new(&[], dataset_path);
                mt.set_date(date);
                mt.save()?;
                Ok(mt)
            }
        }
    }

    pub fn load_current_open_tasks(dataset_path: &Path) -> TaudResult<Vec<TaskInfo>> {
        let mt = Self::load_or_create(&get_current_time(), dataset_path)?;
        Ok(mt.objects()?.into_iter().filter(|t| t.get_state() != "stop").collect())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{create_dir_all, remove_dir_all},
        path::PathBuf,
    };

    use super::*;
    use darkfi::Result;

    const TEST_DATA_PATH: &str = "/tmp/test_tau_data";

    fn get_path() -> Result<PathBuf> {
        remove_dir_all(TEST_DATA_PATH).ok();

        let path = PathBuf::from(TEST_DATA_PATH);

        // mkdir dataset_path if not exists
        create_dir_all(path.join("month"))?;
        create_dir_all(path.join("task"))?;
        Ok(path)
    }

    #[test]
    fn load_and_save_tasks() -> TaudResult<()> {
        let dataset_path = get_path()?;

        // load and save TaskInfo
        ///////////////////////

        let mut task = TaskInfo::new("test_title", "test_desc", None, 0.0, &dataset_path)?;

        task.save()?;

        let t_load = TaskInfo::load(&task.ref_id, &dataset_path)?;

        assert_eq!(task, t_load);

        task.set_title("test_title_2");

        task.save()?;

        let t_load = TaskInfo::load(&task.ref_id, &dataset_path)?;

        assert_eq!(task, t_load);

        // load and save MonthTasks
        ///////////////////////

        let task_tks = vec![];

        let mut mt = MonthTasks::new(&task_tks, &dataset_path);

        mt.save()?;

        let mt_load = MonthTasks::load_or_create(&get_current_time(), &dataset_path)?;

        assert_eq!(mt, mt_load);

        mt.add(&task.ref_id);

        mt.save()?;

        let mt_load = MonthTasks::load_or_create(&get_current_time(), &dataset_path)?;

        assert_eq!(mt, mt_load);

        // activate task
        ///////////////////////

        let task = TaskInfo::new("test_title_3", "test_desc", None, 0.0, &dataset_path)?;

        task.save()?;

        let mt_load = MonthTasks::load_or_create(&get_current_time(), &dataset_path)?;

        assert!(!mt_load.task_tks.contains(&task.ref_id));

        task.activate()?;

        let mt_load = MonthTasks::load_or_create(&get_current_time(), &dataset_path)?;

        assert!(mt_load.task_tks.contains(&task.ref_id));

        remove_dir_all(TEST_DATA_PATH).ok();

        Ok(())
    }
}
