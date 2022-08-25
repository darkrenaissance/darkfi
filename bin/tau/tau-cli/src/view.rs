use std::{fmt::Write, str::FromStr};

use prettytable::{
    cell,
    format::{consts::FORMAT_NO_COLSEP, FormatBuilder, LinePosition, LineSeparator},
    row, table, Cell, Row, Table,
};

use darkfi::{
    util::time::{timestamp_to_date, DateFormat},
    Result,
};
use textwrap::fill;

use crate::{
    filter::apply_filter,
    primitives::{Comment, State, TaskInfo},
    TaskEvent,
};

pub fn print_task_list(tasks: Vec<TaskInfo>, ws: String, filters: Vec<String>) -> Result<()> {
    let mut tasks = tasks;

    let mut table = Table::new();
    table.set_format(
        FormatBuilder::new()
            .padding(1, 1)
            .separators(&[LinePosition::Title], LineSeparator::new('-', ' ', ' ', ' '))
            .build(),
    );
    table.set_titles(row!["ID", "Title", "Project", "Assigned", "Due", "Rank"]);

    for filter in filters {
        apply_filter(&mut tasks, &filter);
    }

    tasks.sort_by(|a, b| b.rank.partial_cmp(&a.rank).unwrap());

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
        } else {
            ("", "", "", "")
        };

        let rank = if let Some(r) = task.rank { r.to_string() } else { "".to_string() };

        table.add_row(Row::new(vec![
            Cell::new(&task.id.to_string()).style_spec(gen_style),
            Cell::new(&task.title).style_spec(gen_style),
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

pub fn print_task_info(taskinfo: TaskInfo) -> Result<()> {
    let due = timestamp_to_date(taskinfo.due.unwrap_or(0), DateFormat::Date);
    let created_at = timestamp_to_date(taskinfo.created_at, DateFormat::DateTime);
    let rank = if let Some(r) = taskinfo.rank { r.to_string() } else { "".to_string() };

    let mut table = table!(
        [Bd => "ref_id", &taskinfo.ref_id],
        ["workspace", &taskinfo.workspace],
        [Bd =>"id", &taskinfo.id.to_string()],
        ["owner", &taskinfo.owner],
        [Bd =>"title", &taskinfo.title],
        ["desc", &taskinfo.desc.to_string()],
        [Bd =>"assign", taskinfo.assign.join(", ")],
        ["project", taskinfo.project.join(", ")],
        [Bd =>"due", due],
        ["rank", rank],
        [Bd =>"created_at", created_at],
        ["current_state", &taskinfo.state]);

    table.set_format(
        FormatBuilder::new()
            .padding(1, 1)
            .separators(&[LinePosition::Title], LineSeparator::new('-', ' ', ' ', ' '))
            .build(),
    );

    table.set_titles(row!["Name", "Value"]);
    table.printstd();

    let (events, timestamps) = &events_as_string(taskinfo.events);
    let mut event_table = table!([events, timestamps]);
    event_table.set_format(*FORMAT_NO_COLSEP);
    event_table.printstd();

    Ok(())
}

pub fn comments_as_string(comments: Vec<Comment>) -> String {
    let mut comments_str = String::new();
    for comment in comments {
        writeln!(comments_str, "{}", comment).unwrap();
    }
    comments_str.pop();
    comments_str
}

pub fn events_as_string(events: Vec<TaskEvent>) -> (String, String) {
    let mut events_str = String::new();
    let mut timestamps_str = String::new();
    let width = 50;
    for event in events {
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
            "due" => {
                writeln!(
                    events_str,
                    "- {} changed due date to {}",
                    event.author,
                    timestamp_to_date(event.content.parse::<i64>().unwrap_or(0), DateFormat::Date)
                )
                .unwrap();
            }
            "comment" => {
                // wrap long comments
                let ev_content =
                    fill(&event.content, textwrap::Options::new(width).subsequent_indent("  "));
                // skip wrapped lines to align timestamp with the first line
                for _ in 1..ev_content.lines().collect::<Vec<&str>>().len() {
                    writeln!(timestamps_str, " ").unwrap();
                }
                writeln!(events_str, "- {} made a comment: {}", event.author, ev_content).unwrap();
            }
            "desc" => {
                // wrap long description
                let ev_content =
                    fill(&event.content, textwrap::Options::new(width).subsequent_indent("  "));
                // skip wrapped lines to align timestamp with the first line
                for _ in 1..ev_content.lines().collect::<Vec<&str>>().len() {
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
