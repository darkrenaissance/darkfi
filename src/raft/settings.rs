use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct RaftSettings {
    //
    // Milliseconds
    //
    pub heartbeat_timeout: u64,
    pub timeout: u64,

    //
    // Seconds
    //
    pub prun_messages_duration: i64,
    pub prun_nodes_ids_duration: i64,
    // must be greater than (timeout * 2)
    pub node_id_timeout: i64,

    //
    // Datastore path
    //
    pub datastore_path: PathBuf,
}

impl Default for RaftSettings {
    fn default() -> Self {
        Self {
            heartbeat_timeout: 500,
            timeout: 3000,
            prun_messages_duration: 120,
            prun_nodes_ids_duration: 120,
            node_id_timeout: 16,
            datastore_path: PathBuf::from(""),
        }
    }
}
