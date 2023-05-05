/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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
    io::{stdin, stdout, Write},
    process::exit,
};

use chrono::{Datelike, Local, TimeZone, Utc};
use log::error;
use serde_json::Value;

use darkfi::Result;

use crate::{
    primitives::{State, TaskInfo},
    util::due_as_timestamp,
};

pub fn apply_filter(tasks: &mut Vec<TaskInfo>, filter: &str) {
    match filter {
        "all" => {}
        // Filter by state.
        _ if filter.contains("state:") => {
            let kv: Vec<&str> = filter.split(':').collect();
            if let Some(state) = Value::from(kv[1]).as_str() {
                match state {
                    "open" => tasks.retain(|task| task.state == State::Open.to_string()),
                    "start" => tasks.retain(|task| task.state == State::Start.to_string()),
                    "pause" => tasks.retain(|task| task.state == State::Pause.to_string()),
                    "stop" => tasks.retain(|task| task.state == State::Stop.to_string()),
                    _ => {
                        error!("Not implemented, states are open,start,pause and stop");
                        exit(1)
                    }
                }
            }
        }

        // Filter by tag
        _ if filter.starts_with('+') => tasks.retain(|task| task.tags.contains(&filter.into())),

        // Filter by month
        _ if filter.contains("month:") => {
            let kv: Vec<&str> = filter.split(':').collect();
            if kv.len() == 2 {
                if let Some(value) = Value::from(kv[1]).as_str() {
                    if value.len() != 4 || value.parse::<u32>().is_err() {
                        error!(
                            "Please provide month date as \"MMYY\" (e.g. 0922 for September 2022)"
                        );
                        exit(1)
                    }
                    let (month, year) =
                        (value[..2].parse::<u32>().unwrap(), value[2..].parse::<i32>().unwrap());

                    let year = year + (Utc::now().year() / 100) * 100;
                    tasks.retain(|task| {
                        let date = task.created_at;
                        let task_date = Utc.timestamp_nanos(date.try_into().unwrap()).date_naive();
                        task_date.month() == month && task_date.year() == year
                    })
                } else {
                    error!("Please provide month date as \"MMYY\" (e.g. 0922 for September 2022)");
                    exit(1)
                }
            }
        }

        // Filter by assignee(s).
        _ if filter.contains("assign:") => {
            let kv: Vec<&str> = filter.split(':').collect();
            if kv.len() == 2 {
                if let Some(value) = Value::from(kv[1]).as_str() {
                    if value.is_empty() {
                        tasks.retain(|task| task.assign.is_empty())
                    } else {
                        tasks.retain(|task| task.assign.contains(&value.to_string()))
                    }
                }
            }
        }

        // Filter by project(s).
        _ if filter.contains("project:") => {
            let kv: Vec<&str> = filter.split(':').collect();
            if kv.len() == 2 {
                if let Some(value) = Value::from(kv[1]).as_str() {
                    if value.is_empty() {
                        tasks.retain(|task| task.project.is_empty())
                    } else {
                        tasks.retain(|task| task.project.contains(&value.to_string()))
                    }
                }
            }
        }

        // Filter by rank.
        _ if filter.contains("rank:") => {
            let kv: Vec<&str> = filter.split(':').collect();
            if kv.len() == 3 {
                let value = kv[2].parse::<f32>().unwrap_or(0.0);
                tasks.retain(|task| {
                    if filter.contains("lt") {
                        task.rank < Some(value)
                    } else if filter.contains("gt") {
                        task.rank > Some(value)
                    } else {
                        true
                    }
                })
            }
            tasks.retain(|task| task.rank.is_none())
        }

        // Filter by due date.
        _ if filter.contains("due:") || filter.contains("due.") => {
            let kv: Vec<&str> = filter.split(':').collect();
            let due_op = if filter.contains('.') {
                let due_op: Vec<&str> = kv[0].split('.').collect();
                due_op[1]
            } else {
                ""
            };

            if kv.len() == 2 {
                if let Some(value) = Value::from(kv[1]).as_str() {
                    if value.is_empty() {
                        tasks.retain(|task| task.due.is_none())
                    } else {
                        let filter_date = if value == "today" {
                            Local::now().date_naive()
                        } else {
                            let due_date = due_as_timestamp(value).unwrap_or(0);
                            Utc.timestamp_nanos(due_date.try_into().unwrap()).date_naive()
                        };

                        tasks.retain(|task| {
                            let date = task.due.unwrap_or(0);
                            let task_date =
                                Utc.timestamp_nanos(date.try_into().unwrap()).date_naive();

                            match due_op {
                                "not" => task_date != filter_date,
                                "after" => task_date > filter_date,
                                "before" => task_date < filter_date,
                                "" | "is" => task_date == filter_date,
                                _ => true,
                            }
                        })
                    }
                } else {
                    error!("Please provide due date as \"DDMM\" (e.g. 2210 for October 22nd)");
                    exit(1)
                }
            }
        }

        _ => {
            println!("No matches.");
            exit(1)
        }
    }
}

pub fn no_filter_warn() {
    let mut s = String::new();
    print!("This command has no filter, and will modify all tasks. Are you sure? (yes/no) ");
    let _ = stdout().flush();
    stdin().read_line(&mut s).unwrap_or(0);
    match s.trim() {
        "y" | "yes" => {}
        _ => {
            println!("Command prevented from running.");
            exit(1)
        }
    }
}

pub fn get_ids(filters: &mut Vec<String>) -> Result<Vec<u64>> {
    let mut vec_ids = vec![];
    let mut matching_id = String::new();
    if let Some(index) = filters.iter().position(|t| {
        t.parse::<u64>().is_ok() || !t.contains(':') && (t.contains(',') || t.contains('-'))
    }) {
        matching_id.push_str(&filters.remove(index));
    }

    match matching_id {
        _ if matching_id.parse::<u64>().is_ok() => {
            let id = matching_id.parse::<u64>().unwrap();
            vec_ids.push(id)
        }
        _ if !matching_id.contains(':') &&
            (matching_id.contains(',') || matching_id.contains('-')) =>
        {
            let num = matching_id.replace(&[',', '-'][..], "");
            if num.parse::<u64>().is_err() {
                error!("Invalid ID number");
                exit(1)
            }
            if matching_id.contains(',') {
                let ids: Vec<&str> = matching_id.split(',').collect();
                for id in ids {
                    if id.contains('-') {
                        let range: Vec<&str> = id.split('-').collect();
                        let range =
                            range[0].parse::<u64>().unwrap()..=range[1].parse::<u64>().unwrap();
                        for rid in range {
                            vec_ids.push(rid)
                        }
                    } else {
                        vec_ids.push(id.parse::<u64>().unwrap())
                    }
                }
            } else if matching_id.contains('-') {
                let range: Vec<&str> = matching_id.split('-').collect();
                let range = range[0].parse::<u64>().unwrap()..=range[1].parse::<u64>().unwrap();
                for rid in range {
                    vec_ids.push(rid)
                }
            }
        }
        _ => {}
    }

    Ok(vec_ids)
}
