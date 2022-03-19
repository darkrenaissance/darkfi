use serde::{Deserialize, Serialize};

pub const CONFIG_FILE_CONTENTS: &[u8] = include_bytes!("../dnetview_config.toml");

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct DnvConfig {
    pub nodes: Vec<IrcNode>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct IrcNode {
    pub name: String,
    pub rpc_url: String,
}
