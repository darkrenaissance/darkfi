use std::path::PathBuf;

use fxhash::FxHashMap;
use log::info;

use darkfi::Result;

#[derive(Clone)]
pub struct Workspace {
    pub encryption: Option<crypto_box::SalsaBox>,
}

impl Workspace {
    pub fn new() -> Result<Self> {
        Ok(Self { encryption: None })
    }
}

/// Parse the configuration file for any configured workspaces and return
/// a map containing said configurations.
pub fn parse_workspaces(config_file: &PathBuf) -> Result<FxHashMap<String, Workspace>> {
    let toml_contents = std::fs::read_to_string(config_file)?;
    let mut ret = FxHashMap::default();

    if let toml::Value::Table(map) = toml::from_str(&toml_contents)? {
        if map.contains_key("workspace") && map["workspace"].is_table() {
            for ws in map["workspace"].as_table().unwrap() {
                info!("Found configuration for workspace {}", ws.0);
                let mut workspace_info = Workspace::new()?;

                if ws.1.as_table().unwrap().contains_key("secret") {
                    // Build the NaCl box
                    let s = ws.1["secret"].as_str().unwrap();
                    let bytes: [u8; 32] = bs58::decode(s).into_vec()?.try_into().unwrap();
                    let secret = crypto_box::SecretKey::from(bytes);
                    let public = secret.public_key();
                    let msg_box = crypto_box::SalsaBox::new(&public, &secret);
                    workspace_info.encryption = Some(msg_box);
                    info!("Instantiated NaCl box for workspace {}", ws.0);
                }

                ret.insert(ws.0.to_string(), workspace_info);
            }
        }
    };

    Ok(ret)
}

pub fn find_free_id(task_ids: &[u32]) -> u32 {
    for i in 1.. {
        if !task_ids.contains(&i) {
            return i
        }
    }
    1
}

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
