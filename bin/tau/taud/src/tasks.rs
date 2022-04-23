use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    error::{TaudError, TaudResult},
    task_info::TaskInfo,
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Tasks {
    task_tks: Vec<String>,
}

impl Tasks {
    pub fn new(task_tks: &[String]) -> Self {
        Self { task_tks: task_tks.to_owned() }
    }

    pub fn add(&mut self, ref_id: &str) {
        if !self.task_tks.contains(&ref_id.into()) {
            self.task_tks.push(ref_id.into());
        }
    }

    pub fn objects(&self, dataset_path: &Path) -> TaudResult<Vec<TaskInfo>> {
        let mut tks: Vec<TaskInfo> = vec![];

        for ref_id in self.task_tks.iter() {
            tks.push(TaskInfo::load(ref_id, dataset_path)?);
        }

        Ok(tks)
    }

    pub fn remove(&mut self, ref_id: &str) {
        if let Some(index) = self.task_tks.iter().position(|t| *t == ref_id) {
            self.task_tks.remove(index);
        }
    }

    fn get_path(dataset_path: &Path, state: &str) -> PathBuf {
        dataset_path.join("log").join(state)
    }

    pub fn save(&self, dataset_path: &Path, state: &str) -> TaudResult<()> {
        crate::util::save::<Self>(&Self::get_path(dataset_path, state), self)
            .map_err(TaudError::Darkfi)
    }

    pub fn load_or_create(dataset_path: &Path, state: &str) -> TaudResult<Self> {
        match crate::util::load::<Self>(&Self::get_path(dataset_path, state)) {
            Ok(mt) => Ok(mt),
            Err(_) => {
                let mt = Self::new(&[]);
                mt.save(dataset_path, state)?;
                Ok(mt)
            }
        }
    }

    pub fn load_current_open_tasks(dataset_path: &Path) -> TaudResult<Vec<TaskInfo>> {
        let mt = Self::load_or_create(dataset_path, "pending")?;
        Ok(mt.objects(dataset_path)?.into_iter().filter(|t| t.get_state() != "stop").collect())
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
        create_dir_all(path.join("log"))?;
        create_dir_all(path.join("task"))?;
        Ok(path)
    }

    #[test]
    fn load_and_save_tasks() -> TaudResult<()> {
        let dataset_path = get_path()?;

        // load and save TaskInfo
        ///////////////////////

        let mut task = TaskInfo::new("test_title", "test_desc", None, 0.0, &dataset_path)?;

        task.save(&dataset_path)?;

        let t_load = TaskInfo::load(&task.ref_id, &dataset_path)?;

        assert_eq!(task, t_load);

        task.set_title("test_title_2");

        task.save(&dataset_path)?;

        let t_load = TaskInfo::load(&task.ref_id, &dataset_path)?;

        assert_eq!(task, t_load);

        // load and save Tasks
        ///////////////////////

        let task_tks = vec![];

        let mut mt = Tasks::new(&task_tks);

        mt.save(&dataset_path, "pending")?;

        let mt_load = Tasks::load_or_create(&dataset_path, "pending")?;

        assert_eq!(mt, mt_load);

        mt.add(&task.ref_id);

        mt.save(&dataset_path, "pending")?;

        let mt_load = Tasks::load_or_create(&dataset_path, "pending")?;

        assert_eq!(mt, mt_load);

        // activate task
        ///////////////////////

        let task = TaskInfo::new("test_title_3", "test_desc", None, 0.0, &dataset_path)?;

        task.save(&dataset_path)?;

        let mt_load = Tasks::load_or_create(&dataset_path, "pending")?;

        assert!(mt_load.task_tks.contains(&task.ref_id));

        remove_dir_all(TEST_DATA_PATH).ok();

        Ok(())
    }
}
