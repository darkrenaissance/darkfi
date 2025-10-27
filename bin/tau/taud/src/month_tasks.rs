/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
};

use chrono::{TimeZone, Utc};
use tinyjson::JsonValue;
use tracing::debug;

use darkfi::util::{
    file::{load_json_file, save_json_file},
    time::Timestamp,
};

use crate::{
    error::{TaudError, TaudResult},
    task_info::TaskInfo,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MonthTasks {
    created_at: Timestamp,
    active_tks: Vec<String>,
    deactive_tks: Vec<String>,
}

impl From<MonthTasks> for JsonValue {
    fn from(mt: MonthTasks) -> JsonValue {
        let active_tks: Vec<JsonValue> =
            mt.active_tks.iter().map(|x| JsonValue::String(x.clone())).collect();

        let deactive_tks: Vec<JsonValue> =
            mt.deactive_tks.iter().map(|x| JsonValue::String(x.clone())).collect();

        JsonValue::Object(HashMap::from([
            ("created_at".to_string(), JsonValue::String(mt.created_at.inner().to_string())),
            ("active_tks".to_string(), JsonValue::Array(active_tks)),
            ("deactive_tks".to_string(), JsonValue::Array(deactive_tks)),
        ]))
    }
}

impl From<JsonValue> for MonthTasks {
    fn from(value: JsonValue) -> MonthTasks {
        let created_at = {
            let u64_str = value["created_at"].get::<String>().unwrap();
            Timestamp::from_u64(u64_str.parse::<u64>().unwrap())
        };

        let active_tks: Vec<String> = value["active_tks"]
            .get::<Vec<JsonValue>>()
            .unwrap()
            .iter()
            .map(|x| x.get::<String>().unwrap().clone())
            .collect();

        let deactive_tks: Vec<String> = value["deactive_tks"]
            .get::<Vec<JsonValue>>()
            .unwrap()
            .iter()
            .map(|x| x.get::<String>().unwrap().clone())
            .collect();

        MonthTasks { created_at, active_tks, deactive_tks }
    }
}

impl MonthTasks {
    pub fn new(active_tks: &[String], deactive_tks: &[String]) -> Self {
        Self {
            created_at: Timestamp::current_time(),
            active_tks: active_tks.to_owned(),
            deactive_tks: deactive_tks.to_owned(),
        }
    }

    pub fn add(&mut self, ref_id: &str) {
        debug!(target: "tau", "MonthTasks::add()");
        if !self.active_tks.contains(&ref_id.into()) {
            self.active_tks.push(ref_id.into());
        }
    }

    pub fn objects(&self, dataset_path: &Path) -> TaudResult<Vec<TaskInfo>> {
        debug!(target: "tau", "MonthTasks::objects()");
        let mut tks: Vec<TaskInfo> = vec![];

        for ref_id in self.active_tks.iter() {
            tks.push(TaskInfo::load(ref_id, dataset_path)?);
        }

        for ref_id in self.deactive_tks.iter() {
            tks.push(TaskInfo::load(ref_id, dataset_path)?);
        }

        Ok(tks)
    }

    pub fn remove(&mut self, ref_id: &str) {
        debug!(target: "tau", "MonthTasks::remove()");
        if self.active_tks.contains(&ref_id.to_string()) {
            if let Some(index) = self.active_tks.iter().position(|t| *t == ref_id) {
                self.deactive_tks.push(self.active_tks.remove(index));
            }
        } else {
            self.deactive_tks.push(ref_id.to_owned());
        }
    }

    pub fn set_date(&mut self, date: &Timestamp) {
        debug!(target: "tau", "MonthTasks::set_date()");
        self.created_at = *date;
    }

    fn get_path(date: &Timestamp, dataset_path: &Path) -> PathBuf {
        debug!(target: "tau", "MonthTasks::get_path()");
        dataset_path.join("month").join(
            Utc.timestamp_opt(date.inner().try_into().unwrap(), 0)
                .unwrap()
                .format("%m%y")
                .to_string(),
        )
    }

    pub fn save(&self, dataset_path: &Path) -> TaudResult<()> {
        debug!(target: "tau", "MonthTasks::save()");
        let mt: JsonValue = self.clone().into();
        save_json_file(&Self::get_path(&self.created_at, dataset_path), &mt, true)
            .map_err(TaudError::Darkfi)
    }

    fn get_all(dataset_path: &Path) -> io::Result<Vec<PathBuf>> {
        debug!(target: "tau", "MonthTasks::get_all()");

        let mut entries = fs::read_dir(dataset_path.join("month"))?
            .map(|res| res.map(|e| e.path()))
            .collect::<Result<Vec<_>, io::Error>>()?;

        entries.sort();

        Ok(entries)
    }

    fn create(date: &Timestamp, dataset_path: &Path) -> TaudResult<Self> {
        debug!(target: "tau", "MonthTasks::create()");

        let mut mt = Self::new(&[], &[]);
        mt.set_date(date);
        mt.save(dataset_path)?;
        Ok(mt)
    }

    pub fn load_or_create(date: Option<&Timestamp>, dataset_path: &Path) -> TaudResult<Self> {
        debug!(target: "tau", "MonthTasks::load_or_create()");

        // if a date is given we load that date's month tasks
        // if not, we load tasks from all months
        match date {
            Some(date) => match load_json_file(&Self::get_path(date, dataset_path)) {
                Ok(mt) => Ok(mt.into()),
                Err(_) => Self::create(date, dataset_path),
            },
            None => {
                let path_all = Self::get_all(dataset_path).unwrap_or_default();

                let mut loaded_mt = Self::new(&[], &[]);

                for path in path_all {
                    let mt = load_json_file(&path)?;
                    let mt: MonthTasks = mt.into();
                    loaded_mt.created_at = mt.created_at;
                    for tks in mt.active_tks {
                        if !loaded_mt.active_tks.contains(&tks) {
                            loaded_mt.active_tks.push(tks)
                        }
                    }
                    for dtks in mt.deactive_tks {
                        if !loaded_mt.deactive_tks.contains(&dtks) {
                            loaded_mt.deactive_tks.push(dtks)
                        }
                    }
                }
                Ok(loaded_mt)
            }
        }
    }

    pub fn load_current_tasks(
        dataset_path: &Path,
        ws: String,
        all: bool,
    ) -> TaudResult<Vec<TaskInfo>> {
        let mt = Self::load_or_create(None, dataset_path)?;

        if all {
            Ok(mt.objects(dataset_path)?.into_iter().filter(|t| t.workspace == ws).collect())
        } else {
            Ok(mt
                .objects(dataset_path)?
                .into_iter()
                .filter(|t| t.get_state() != "stop" && t.workspace == ws)
                .collect())
        }
    }

    pub fn load_stop_tasks(
        dataset_path: &Path,
        ws: String,
        date: Option<&Timestamp>,
    ) -> TaudResult<Vec<TaskInfo>> {
        let mt = Self::load_or_create(date, dataset_path)?;
        Ok(mt
            .objects(dataset_path)?
            .into_iter()
            .filter(|t| t.get_state() == "stop" && t.workspace == ws)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use std::fs::{create_dir_all, remove_dir_all};

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

        let mut task = TaskInfo::new(
            "darkfi".to_string(),
            "test_title",
            "test_desc",
            "NICKNAME",
            None,
            Some(0.0),
            Timestamp::current_time(),
            None,
        )?;

        task.save(&dataset_path)?;

        let t_load = TaskInfo::load(&task.ref_id, &dataset_path)?;

        assert_eq!(task, t_load);

        task.set_title("test_title_2");

        task.save(&dataset_path)?;

        let t_load = TaskInfo::load(&task.ref_id, &dataset_path)?;

        assert_eq!(task, t_load);

        // load and save MonthTasks
        ///////////////////////

        let task_tks = vec![];

        let mut mt = MonthTasks::new(&task_tks, &[]);

        mt.save(&dataset_path)?;

        let mt_load = MonthTasks::load_or_create(Some(&Timestamp::current_time()), &dataset_path)?;

        assert_eq!(mt, mt_load);

        mt.add(&task.ref_id);

        mt.save(&dataset_path)?;

        let mt_load = MonthTasks::load_or_create(Some(&Timestamp::current_time()), &dataset_path)?;

        assert_eq!(mt, mt_load);

        // activate task
        ///////////////////////

        let task = TaskInfo::new(
            "darkfi".to_string(),
            "test_title_3",
            "test_desc",
            "NICKNAME",
            None,
            Some(0.0),
            Timestamp::current_time(),
            None,
        )?;

        task.save(&dataset_path)?;

        let mt_load = MonthTasks::load_or_create(Some(&Timestamp::current_time()), &dataset_path)?;

        assert!(mt_load.active_tks.contains(&task.ref_id));

        remove_dir_all(TEST_DATA_PATH).ok();

        Ok(())
    }
}
