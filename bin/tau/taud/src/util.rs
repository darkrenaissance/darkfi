use std::{fs::File, io::BufReader, path::Path};

use chrono::Utc;
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use darkfi::{
    util::serial::{SerialDecodable, SerialEncodable},
    Result,
};

pub fn random_ref_id() -> String {
    thread_rng().sample_iter(&Alphanumeric).take(30).map(char::from).collect()
}

pub fn get_current_time() -> Timestamp {
    Timestamp(Utc::now().timestamp())
}

pub fn find_free_id(task_ids: &[u32]) -> u32 {
    for i in 1.. {
        if !task_ids.contains(&i) {
            return i
        }
    }
    1
}

pub fn load<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    let value: T = serde_json::from_reader(reader)?;
    Ok(value)
}

pub fn save<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, value)?;
    Ok(())
}

#[derive(
    Clone, Debug, Serialize, Deserialize, SerialEncodable, SerialDecodable, PartialEq, PartialOrd,
)]
pub struct Timestamp(pub i64);

#[cfg(test)]
mod tests {
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
}
