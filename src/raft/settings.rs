use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct RaftSettings {
    // the leader duration for sending heartbeat; in milliseconds
    pub heartbeat_timeout: u64,

    // the duration for electing new leader; in seconds
    pub timeout: u64,

    // the duration for sending id to other nodes; in seconds
    pub id_timeout: u64,

    // this duration used to clean up hashmaps; in seconds
    pub prun_duration: i64,

    // Datastore path
    pub datastore_path: PathBuf,
}

impl Default for RaftSettings {
    fn default() -> Self {
        Self {
            heartbeat_timeout: 500,
            timeout: 6,
            id_timeout: 12,
            prun_duration: 240,
            datastore_path: PathBuf::from(""),
        }
    }
}
