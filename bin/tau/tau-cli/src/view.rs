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
    primitives::{Comment, State, TaskInfo},
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
            "tags" => {
                writeln!(events_str, "- {} changed tags to {}", event.author, event.content)
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
                for _ in 1..ev_content.lines().count() {
                    writeln!(timestamps_str, " ").unwrap();
                }
                writeln!(events_str, "- {} made a comment: {}", event.author, ev_content).unwrap();
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
