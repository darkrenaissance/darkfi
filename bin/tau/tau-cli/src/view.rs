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

use std::{cmp::Ordering, fmt::Write, str::FromStr};

use prettytable::{
    format::{consts::FORMAT_NO_COLSEP, FormatBuilder, LinePosition, LineSeparator},
    row, table, Cell, Row, Table,
};
use textwrap::fill;

use darkfi::{
    util::time::{timestamp_to_date, DateFormat},
    Result,
};

use crate::{
    primitives::{State, TaskInfo},
    TaskEvent,
};

pub fn print_task_list(tasks: Vec<TaskInfo>, ws: String) -> Result<()> {
    let mut tasks = tasks;

    let mut table = Table::new();
    table.set_format(
        FormatBuilder::new()
            .padding(1, 1)
            .separators(&[LinePosition::Title], LineSeparator::new('-', ' ', ' ', ' '))
            .build(),
    );
    table.set_titles(row!["ID", "Title", "Tags", "Project", "Assigned", "Due", "Rank"]);

    // group tasks by state.
    tasks.sort_by_key(|task| task.state.clone());

    // sort tasks by there rank only if they are not stopped.
    tasks.sort_by(|a, b| {
        if a.state != "stop" && b.state != "stop" {
            b.rank.partial_cmp(&a.rank).unwrap()
        } else {
            // because sort_by does not reorder equal elements
            Ordering::Equal
        }
    });

    let mut min_rank = None;
    let mut max_rank = None;

    if let Some(first) = tasks.first() {
        max_rank = first.rank;
    }

    if let Some(last) = tasks.last() {
        min_rank = last.rank;
    }

    for task in tasks {
        let state = State::from_str(&task.state.clone())?;

        let (max_style, min_style, mid_style, gen_style) = if state.is_start() {
            ("bFg", "Fc", "Fg", "Fg")
        } else if state.is_pause() {
            ("iFYBd", "iFYBd", "iFYBd", "iFYBd")
        } else if state.is_stop() {
            ("Fr", "Fr", "Fr", "Fr")
        } else {
            ("", "", "", "")
        };

        let rank = if let Some(r) = task.rank { r.to_string() } else { "".to_string() };
        let mut print_tags = vec![];
        for tag in &task.tags {
            let t = tag.replace('+', "");
            print_tags.push(t)
        }

        table.add_row(Row::new(vec![
            Cell::new(&task.id.to_string()).style_spec(gen_style),
            Cell::new(&task.title).style_spec(gen_style),
            Cell::new(&print_tags.join(", ")).style_spec(gen_style),
            Cell::new(&task.project.join(", ")).style_spec(gen_style),
            Cell::new(&task.assign.join(", ")).style_spec(gen_style),
            Cell::new(&timestamp_to_date(task.due.unwrap_or(0), DateFormat::Date))
                .style_spec(gen_style),
            if task.rank == max_rank {
                Cell::new(&rank).style_spec(max_style)
            } else if task.rank == min_rank {
                Cell::new(&rank).style_spec(min_style)
            } else {
                Cell::new(&rank).style_spec(mid_style)
            },
        ]));
    }

    let workspace = format!("Workspace: {}", ws);
    let mut ws_table = table!([workspace]);
    ws_table.set_format(
        FormatBuilder::new()
            .padding(1, 1)
            .separators(&[LinePosition::Bottom], LineSeparator::new('-', ' ', ' ', ' '))
            .build(),
    );

    ws_table.printstd();
    table.printstd();
    Ok(())
}

pub fn taskinfo_table(taskinfo: TaskInfo) -> Result<Table> {
    let due = timestamp_to_date(taskinfo.due.unwrap_or(0), DateFormat::Date);
    let created_at = timestamp_to_date(taskinfo.created_at, DateFormat::DateTime);
    let rank = if let Some(r) = taskinfo.rank { r.to_string() } else { "".to_string() };

    let mut table = table!(
         [Bd => "ref_id", &taskinfo.ref_id],
         ["workspace", &taskinfo.workspace],
         [Bd =>"id", &taskinfo.id.to_string()],
         ["owner", &taskinfo.owner],
         [Bd =>"title", &taskinfo.title],
         ["tags", &taskinfo.tags.join(", ")],
         [Bd =>"desc", &taskinfo.desc.to_string()],
         ["assign", taskinfo.assign.join(", ")],
         [Bd =>"project", taskinfo.project.join(", ")],
         ["due", due],
         [Bd =>"rank", rank],
         ["created_at", created_at],
         [Bd =>"current_state", &taskinfo.state]);

    table.set_format(
        FormatBuilder::new()
            .padding(1, 1)
            .separators(&[LinePosition::Title], LineSeparator::new('-', ' ', ' ', ' '))
            .build(),
    );

    table.set_titles(row!["Name", "Value"]);
    Ok(table)
}

pub fn events_table(taskinfo: TaskInfo) -> Result<Table> {
    let (events, timestamps) = &events_as_string(taskinfo.events);
    let mut events_table = table!([events, timestamps]);
    events_table.set_format(*FORMAT_NO_COLSEP);
    events_table.set_titles(row!["Events"]);
    Ok(events_table)
}

pub fn comments_table(taskinfo: TaskInfo) -> Result<Table> {
    let (events, timestamps) = &comments_as_string(taskinfo.events);
    let mut comments_table = table!([events, timestamps]);
    comments_table.set_format(*FORMAT_NO_COLSEP);
    comments_table.set_titles(row!["Comments"]);

    Ok(comments_table)
}

pub fn print_task_info(taskinfo: TaskInfo) -> Result<()> {
    let table = taskinfo_table(taskinfo.clone())?;
    table.printstd();

    let events_table = events_table(taskinfo.clone())?;
    events_table.printstd();

    let comments_table = comments_table(taskinfo)?;
    comments_table.printstd();

    println!();

    Ok(())
}

pub fn events_as_string(events: Vec<TaskEvent>) -> (String, String) {
    let mut events_str = String::new();
    let mut timestamps_str = String::new();
    let width = 50;
    for event in events {
        if event.action.as_str() == "comment" {
            continue
        }
        writeln!(timestamps_str, "{}", event.timestamp).unwrap();
        match event.action.as_str() {
            "title" => {
                writeln!(events_str, "- {} changed title to {}", event.author, event.content)
                    .unwrap();
            }
            "rank" => {
                writeln!(events_str, "- {} changed rank to {}", event.author, event.content)
                    .unwrap();
            }
            "state" => {
                writeln!(events_str, "- {} changed state to {}", event.author, event.content)
                    .unwrap();
            }
            "assign" => {
                writeln!(events_str, "- {} assigned {}", event.author, event.content).unwrap();
            }
            "project" => {
                writeln!(events_str, "- {} changed project to {}", event.author, event.content)
                    .unwrap();
            }
            "tags" => {
                writeln!(events_str, "- {} changed tags to {}", event.author, event.content)
                    .unwrap();
            }
            "due" => {
                writeln!(
                    events_str,
                    "- {} changed due date to {}",
                    event.author,
                    timestamp_to_date(event.content.parse::<u64>().unwrap_or(0), DateFormat::Date)
                )
                .unwrap();
            }
            "desc" => {
                // wrap long description
                let ev_content =
                    fill(&event.content, textwrap::Options::new(width).subsequent_indent("  "));
                // skip wrapped lines to align timestamp with the first line
                for _ in 1..ev_content.lines().count() {
                    writeln!(timestamps_str, " ").unwrap();
                }
                writeln!(events_str, "- {} changed description to: {}", event.author, ev_content)
                    .unwrap();
            }
            _ => {}
        }
    }
    (events_str, timestamps_str)
}

pub fn comments_as_string(events: Vec<TaskEvent>) -> (String, String) {
    let mut events_str = String::new();
    let mut timestamps_str = String::new();
    let width = 50;
    for event in events {
        if event.action.as_str() != "comment" {
            continue
        }
        writeln!(timestamps_str, "{}", event.timestamp).unwrap();
        if event.action.as_str() == "comment" {
            // wrap long comments
            let ev_content =
                fill(&event.content, textwrap::Options::new(width).subsequent_indent("    "));
            // skip wrapped lines to align timestamp with the first line
            for _ in 1..ev_content.lines().count() {
                writeln!(timestamps_str, " ").unwrap();
            }
            writeln!(events_str, "{}>  {}", event.author, ev_content).unwrap();
        }
    }
    (events_str, timestamps_str)
}
