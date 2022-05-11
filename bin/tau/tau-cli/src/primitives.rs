use serde::{Deserialize, Serialize};

use darkfi::util::Timestamp;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskInfo {
    pub ref_id: String,
    pub id: u32,
    pub title: String,
    pub desc: String,
    pub owner: String,
    pub assign: Vec<String>,
    pub project: Vec<String>,
    pub due: Option<i64>,
    pub rank: f32,
    pub created_at: i64,
    pub events: Vec<TaskEvent>,
    pub comments: Vec<Comment>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskEvent {
    pub action: String,
    pub timestamp: Timestamp,
}

impl std::fmt::Display for TaskEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "action: {}, timestamp: {}", self.action, self.timestamp)
    }
}

impl Default for TaskEvent {
    fn default() -> Self {
        TaskEvent {
            action: "open".to_string(),
            timestamp: Timestamp(chrono::offset::Local::now().timestamp()),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Comment {
    content: String,
    author: String,
    timestamp: Timestamp,
}

impl std::fmt::Display for Comment {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{} author: {}, content: {} ", self.timestamp, self.author, self.content)
    }
}
