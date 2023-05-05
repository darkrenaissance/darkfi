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

use std::collections::HashMap;

use chrono::{Datelike, Duration, NaiveDate, Utc};
use colored::Colorize;
use term_grid::{Cell, Direction, Filling, Grid, GridOptions};

use darkfi::{util::time::DateTime, Error, Result};

use crate::primitives::{TaskEvent, TaskInfo};

// Red
const NO_TASK_SCALE: u8 = 50;
const MIN_SCALE: usize = 90;
const MAX_SCALE: usize = 255;
const INCREASE_FACTOR: usize = 25;

// Green
const GREEN: u8 = 40;
// Blue
const BLUE: u8 = 50;

/// Log drawdown gets all assignees of tasks, stores a vec of stopped tasks for each
/// assignee in a hashmap, draw a heatmap of how many stopped tasks in each day of the
/// specified month and assignee.
pub fn drawdown(date: String, tasks: Vec<TaskInfo>, assignee: Option<String>) -> Result<()> {
    let mut ret = HashMap::new();
    let assignees = assignees(tasks.clone());

    if assignee.is_none() {
        println!("Assignees of this month's tasks are: {}", assignees.join(", "));
        return Ok(())
    }

    let asgn = assignee.unwrap();

    if !assignees.contains(&asgn) {
        eprintln!("Assignee {} not found, run \"tau log -h\" for more information.", asgn);
        return Ok(())
    }

    for assignee in assignees {
        let stopped_tasks = tasks
            .clone()
            .into_iter()
            .filter(|task| {
                if task.assign.is_empty() {
                    task.owner == asgn
                } else {
                    task.assign.contains(&asgn)
                }
            })
            .collect::<Vec<TaskInfo>>();
        ret.insert(assignee, stopped_tasks);
    }

    let mut naivedate = to_naivedate(date.clone())?;

    println!("log drawdown for {} in {}", asgn, naivedate.format("%b %Y"));

    let fdow = if naivedate.month() == 2 && !is_leap_year(naivedate.year()) {
        ["   ", "1 ", "8 ", "15", "22", " "]
    } else {
        ["   ", "1 ", "8 ", "15", "22", "29"]
    };

    // Print first day of each week horizontally.
    let mut dow_grid =
        Grid::new(GridOptions { direction: Direction::LeftToRight, filling: Filling::Spaces(1) });
    if ret.contains_key(&asgn) {
        for i in fdow {
            let cell = Cell::from(i);
            dow_grid.add(cell)
        }
        let grid_display = dow_grid.fit_into_rows(1);
        print!("{}", grid_display);
    }

    let mut grid =
        Grid::new(GridOptions { direction: Direction::TopToBottom, filling: Filling::Spaces(1) });

    let days_in_month = get_days_from_month(date)? as u32;

    if ret.contains_key(&asgn) {
        for _ in 0..7 {
            let dow = naivedate.weekday().to_string();
            let wcell = Cell::from(dow);
            grid.add(wcell);
            naivedate += Duration::days(1);
        }
        for day in 1..=days_in_month {
            let owner_stopped_tasks = ret.get(&asgn).unwrap().to_owned();
            let date_tasks: Vec<TaskInfo> = owner_stopped_tasks
                .into_iter()
                .filter(|t| {
                    // last event is always state stop
                    let event_date = DateTime::from_timestamp(
                        t.events.last().unwrap_or(&TaskEvent::default()).timestamp.0,
                        0,
                    );
                    // let event_date = Utc.timestamp_nanos(
                    //     t.events
                    //         .last()
                    //         .unwrap_or(&TaskEvent::default())
                    //         .timestamp
                    //         .0
                    //         .try_into()
                    //         .unwrap(),
                    // );
                    event_date.day == day
                })
                .collect();

            let red_scale = if date_tasks.is_empty() {
                NO_TASK_SCALE
            } else {
                ((date_tasks.len() * INCREASE_FACTOR) + MIN_SCALE).clamp(MIN_SCALE, MAX_SCALE) as u8
            };

            let cell = Cell::from("▀▀".truecolor(red_scale, GREEN, BLUE));
            grid.add(cell)
        }
    }

    let grid_display = grid.fit_into_rows(7);
    println!("{}", grid_display);

    Ok(())
}

fn is_leap_year(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn helper_parse_func(date: String) -> Result<(u32, i32)> {
    if date.len() != 4 || date.parse::<u32>().is_err() {
        return Err(Error::MalformedPacket)
    }
    let (month, year) = (date[..2].parse::<u32>().unwrap(), date[2..].parse::<i32>().unwrap());
    let year = year + (Utc::now().year() / 100) * 100;

    Ok((month, year))
}

pub fn to_naivedate(date: String) -> Result<NaiveDate> {
    let (month, year) = helper_parse_func(date)?;
    Ok(NaiveDate::from_ymd_opt(year, month, 1).unwrap())
}

fn get_days_from_month(date: String) -> Result<i64> {
    let (month, year) = helper_parse_func(date)?;

    Ok(NaiveDate::from_ymd_opt(
        match month {
            12 => year + 1,
            _ => year,
        },
        match month {
            12 => 1,
            _ => month + 1,
        },
        1,
    )
    .unwrap()
    .signed_duration_since(NaiveDate::from_ymd_opt(year, month, 1).unwrap())
    .num_days())
}

fn assignees(tasks: Vec<TaskInfo>) -> Vec<String> {
    let mut assignees = vec![];
    for task in tasks {
        // if task is stopped with no assignee specified we give credit to the owner
        if task.assign.is_empty() {
            if !assignees.contains(&task.owner) {
                assignees.push(task.owner)
            }
        } else {
            for assignee in task.assign {
                if !assignees.contains(&assignee) {
                    assignees.push(assignee)
                }
            }
        }
    }

    assignees
}
