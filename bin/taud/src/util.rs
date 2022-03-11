use std::{fs::File, io::BufReader, path::PathBuf};

use clap::Parser;
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use darkfi::{util::cli::UrlConfig, Result};

pub const CONFIG_FILE_CONTENTS: &[u8] = include_bytes!("../taud_config.toml");

pub fn random_ref_id() -> String {
    thread_rng().sample_iter(&Alphanumeric).take(30).map(char::from).collect()
}

pub fn find_free_id(tasks_ids: &Vec<u32>) -> u32 {
    for i in 1.. {
        if !tasks_ids.contains(&i) {
            return i
        }
    }
    1
}

pub fn load<T: DeserializeOwned>(path: &PathBuf) -> Result<T> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    let value: T = serde_json::from_reader(reader)?;
    Ok(value)
}

pub fn save<T: Serialize>(path: &PathBuf, value: &T) -> Result<()> {
    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, value)?;
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub dataset_path: PathBuf,
}

impl Default for Settings {
    fn default() -> Self {
        Self { dataset_path: PathBuf::from("") }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Timestamp(pub String);

/// taud cli
#[derive(Parser)]
#[clap(name = "taud")]
pub struct CliTaud {
    /// Sets a custom config file
    #[clap(short, long)]
    pub config: Option<String>,
    /// Increase verbosity
    #[clap(short, parse(from_occurrences))]
    pub verbose: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TauConfig {
    /// path to dataset
    pub dataset_path: String,
    /// Path to DER-formatted PKCS#12 archive. (used only with tls listener url)
    pub tls_identity_path: String,
    /// The address where taud should bind its RPC socket
    pub rpc_listener_url: UrlConfig,
}

#[cfg(test)]
mod tests {
    use std::fs::create_dir_all;

    use chrono::Utc;

    use crate::{month_tasks::MonthTasks, task_info::TaskInfo};

    use super::*;

    #[test]
    fn find_free_id_test() -> Result<()> {
        let mut ids: Vec<u32> = vec![1, 3, 8, 9, 10, 3];
        let ids_empty: Vec<u32> = vec![];
        let ids_duplicate: Vec<u32> = vec![1; 100];

        let find_id = find_free_id(&ids);

        assert_eq!(find_id, 2);

        ids.push(find_id);

        assert_eq!(find_free_id(&ids), 4);

        assert_eq!(find_free_id(&ids_empty), 1);

        assert_eq!(find_free_id(&ids_duplicate), 2);

        Ok(())
    }

    #[test]
    fn load_and_save_data() -> Result<()> {
        let path = PathBuf::from("/tmp/test_tau_data");

        // mkdir dataset_path if not exists
        create_dir_all(path.join("month"))?;
        create_dir_all(path.join("task"))?;

        // test with MonthTasks
        ///////////////////////
        let mt_path = path.join("month");
        let mt_path = mt_path.join("022");

        let settings = Settings { dataset_path: path.clone() };
        let task_tks = vec![];
        let created_at = Timestamp(Utc::now().to_string());

        let mut mt = MonthTasks { created_at, task_tks, settings };

        save::<MonthTasks>(&mt_path, &mt)?;

        let mt_load = load::<MonthTasks>(&mt_path)?;
        assert_eq!(mt, mt_load);

        mt.add("test_hash");

        save::<MonthTasks>(&mt_path, &mt)?;

        let mt_load = load::<MonthTasks>(&mt_path)?;
        assert_eq!(mt, mt_load);

        // test with TaskInfo
        ///////////////////////
        let t_path = path.join("task");
        let t_path = t_path.join("test_hash");

        let mut task = TaskInfo::new("test_title", "test_desc", None, 0);

        save::<TaskInfo>(&t_path, &task)?;

        let t_load = load::<TaskInfo>(&t_path)?;
        assert_eq!(task, t_load);

        task.title = "test_title_2".into();

        save::<TaskInfo>(&t_path, &task)?;

        let t_load = load::<TaskInfo>(&t_path)?;
        assert_eq!(task, t_load);

        Ok(())
    }
}
