use chrono::{Datelike, NaiveDate, NaiveDateTime};
use serde_json::Value;

use crate::{primitives::TaskInfo, TaskEvent};

/// Helper function to check task's state
fn check_task_state(task: &TaskInfo, state: &str) -> bool {
    let last_state = task.events.last().unwrap_or(&TaskEvent::default()).action.clone();
    state == last_state
}

pub fn apply_filter(tasks: &mut Vec<TaskInfo>, filter: &str) {
    match filter {
        "open" => tasks.retain(|task| check_task_state(task, "open")),
        "pause" => tasks.retain(|task| check_task_state(task, "pause")),

        _ if filter.len() == 4 && filter.parse::<u32>().is_ok() => {
            let (month, year) =
                (filter[..2].parse::<u32>().unwrap(), filter[2..].parse::<i32>().unwrap());

            let year = year + 2000;
            tasks.retain(|task| {
                let date = task.created_at;
                let task_date = NaiveDateTime::from_timestamp(date, 0).date();
                let filter_date = NaiveDate::from_ymd(year, month, 1);
                task_date.month() == filter_date.month() && task_date.year() == filter_date.year()
            })
        }

        _ if filter.contains("assign:") => {
            let kv: Vec<&str> = filter.split(':').collect();
            if kv.len() == 2 {
                if let Some(value) = Value::from(kv[1]).as_str() {
                    tasks.retain(|task| task.assign.contains(&value.to_string()))
                }
            }
        }

        _ if filter.contains("project:") => {
            let kv: Vec<&str> = filter.split(':').collect();
            if kv.len() == 2 {
                if let Some(value) = Value::from(kv[1]).as_str() {
                    tasks.retain(|task| task.project.contains(&value.to_string()))
                }
            }
        }

        _ if filter.contains("rank:") => {
            let kv: Vec<&str> = filter.split(':').collect();
            if kv.len() == 3 {
                let value = kv[2].parse::<f32>().unwrap_or(0.0);
                tasks.retain(|task| {
                    if filter.contains("lt") {
                        task.rank < value
                    } else if filter.contains("gt") {
                        task.rank > value
                    } else {
                        true
                    }
                })
            }
        }

        _ => {}
    }
}
