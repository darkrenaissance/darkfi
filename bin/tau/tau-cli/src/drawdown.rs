use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Utc};
use colored::Colorize;
use fxhash::FxHashMap;
use term_grid::{Cell, Direction, Filling, Grid, GridOptions};

use darkfi::{Error, Result};

use crate::primitives::{TaskEvent, TaskInfo};

pub fn drawdown(date: String, tasks: Vec<TaskInfo>, owner: String) -> Result<()> {
    let mut ret = FxHashMap::default();
    let all_owners = owners(tasks.clone());

    for owner in all_owners {
        let stopped_tasks = tasks
            .clone()
            .into_iter()
            .filter(|t| t.state == "stop" && t.owner == owner)
            .collect::<Vec<TaskInfo>>();
        ret.insert(owner, stopped_tasks);
    }

    let mut naivedate = to_naivedate(date.clone())?;

    println!("log drawdown for {} in {}", owner, naivedate.format("%b %Y").to_string());

    let fdow = if naivedate.month() == 2 && !is_leap_year(naivedate.year()) {
        ["   ", "1 ", "8 ", "15", "22", " "]
    } else {
        ["   ", "1 ", "8 ", "15", "22", "29"]
    };

    // Print first day of each week horizontally.
    let mut dow_grid =
        Grid::new(GridOptions { direction: Direction::LeftToRight, filling: Filling::Spaces(1) });
    if ret.contains_key(&owner) {
        for i in fdow {
            let cell = Cell::from(i);
            dow_grid.add(cell)
        }
        let grid_display = dow_grid.fit_into_rows(1);
        print!("{}", grid_display);
    }

    let mut grid =
        Grid::new(GridOptions { direction: Direction::TopToBottom, filling: Filling::Spaces(1) });

    let days_in_month = get_days_from_month(date) as u32;

    if ret.contains_key(&owner) {
        for _ in 0..7 {
            let dow = naivedate.weekday().to_string();
            let wcell = Cell::from(dow);
            grid.add(wcell);
            naivedate = naivedate + Duration::days(1);
        }
        for day in 1..=days_in_month {
            let owner_stopped_tasks = ret.get(&owner).unwrap().to_owned();
            let date_tasks: Vec<TaskInfo> = owner_stopped_tasks
                .into_iter()
                .filter(|t| {
                    // last event is always state stop
                    let event_date = NaiveDateTime::from_timestamp(
                        t.events.last().unwrap_or(&TaskEvent::default()).timestamp.0,
                        0,
                    );
                    event_date.day() == day
                })
                .collect();

            let red_scale = if date_tasks.is_empty() {
                50
            } else {
                ((date_tasks.len() * 25) + 90).clamp(90, 255) as u8
            };

            let cell = Cell::from("▀▀".truecolor(red_scale, 40, 50));
            grid.add(cell)
        }
    }

    let grid_display = grid.fit_into_rows(7);
    println!("{}", grid_display);

    Ok(())
}

fn is_leap_year(year: i32) -> bool {
    return year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

pub fn to_naivedate(date: String) -> Result<NaiveDate> {
    if date.len() != 4 || date.parse::<u32>().is_err() {
        return Err(Error::MalformedPacket)
    }
    let (month, year) = (date[..2].parse::<u32>().unwrap(), date[2..].parse::<i32>().unwrap());
    let year = year + (Utc::today().year() / 100) * 100;

    Ok(NaiveDate::from_ymd(year, month, 1))
}

fn get_days_from_month(date: String) -> i64 {
    let (month, year) = (date[..2].parse::<u32>().unwrap(), date[2..].parse::<i32>().unwrap());
    let year = year + (Utc::today().year() / 100) * 100;

    NaiveDate::from_ymd(
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
    .signed_duration_since(NaiveDate::from_ymd(year, month, 1))
    .num_days()
}

fn owners(tasks: Vec<TaskInfo>) -> Vec<String> {
    let mut owners = vec![];
    for task in tasks {
        if !owners.contains(&task.owner) {
            owners.push(task.owner)
        }
    }

    owners
}
