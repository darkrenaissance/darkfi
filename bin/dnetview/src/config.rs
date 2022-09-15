use serde::{Deserialize, Serialize};

pub const CONFIG_FILE: &str = "dnetview_config.toml";
pub const CONFIG_FILE_CONTENTS: &[u8] = include_bytes!("../dnetview_config.toml");

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DnvConfig {
    pub nodes: Vec<Node>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Node {
    pub name: String,
    pub rpc_url: String,
    pub node_type: NodeType,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum NodeType {
    LILITH,
    NORMAL,
}
