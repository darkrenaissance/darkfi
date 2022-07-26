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

use crate::{
    filter::apply_filter,
    primitives::{Comment, State, TaskInfo},
    TaskEvent,
};

pub fn print_task_list(tasks: Vec<TaskInfo>, filters: Vec<String>) -> Result<()> {
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

    let mut min_rank = 0.0;
    let mut max_rank = 0.0;

    if let Some(first) = tasks.first() {
        max_rank = first.rank;
    }

    if let Some(last) = tasks.last() {
        min_rank = last.rank;
    }

    let workspace = if tasks.first().is_some() {
        format!("Workspace: {}", tasks.first().unwrap().workspace.clone())
    } else {
        format!("Workspace: ")
    };

    for task in tasks {
        let state = task.events.last().unwrap_or(&TaskEvent::default()).action.clone();
        let state = State::from_str(&state)?;

        let (max_style, min_style, mid_style, gen_style) = if state.is_start() {
            ("bFg", "Fc", "Fg", "Fg")
        } else if state.is_pause() {
            ("iFYBd", "iFYBd", "iFYBd", "iFYBd")
        } else {
            ("", "", "", "")
        };

        let rank = task.rank.to_string();

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

    let mut ws_table = table!([Fb => workspace]);
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
    let current_state = &taskinfo.events.last().unwrap_or(&TaskEvent::default()).action.clone();
    let due = timestamp_to_date(taskinfo.due.unwrap_or(0), DateFormat::Date);
    let created_at = timestamp_to_date(taskinfo.created_at, DateFormat::DateTime);

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
        ["rank", &taskinfo.rank.to_string()],
        [Bd =>"created_at", created_at],
        ["current_state", current_state]);

    table.set_format(
        FormatBuilder::new()
            .padding(1, 1)
            .separators(&[LinePosition::Title], LineSeparator::new('-', ' ', ' ', ' '))
            .build(),
    );

    table.set_titles(row!["Name", "Value"]);
    table.printstd();

    let mut event_table = table!(["events", &events_as_string(taskinfo.events)]);
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

pub fn events_as_string(events: Vec<TaskEvent>) -> String {
    let mut events_str = String::new();
    for event in events {
        writeln!(events_str, "State changed to {} at {}", event.action, event.timestamp).unwrap();
    }
    events_str
}
