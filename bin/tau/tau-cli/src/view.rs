use prettytable::{cell, format, row, table, Cell, Row, Table};

use darkfi::{util::time::timestamp_to_date, Result};

use super::{
    filter::apply_filter,
    primitives::{Comment, TaskEvent, TaskInfo},
};

pub fn print_list_of_task(tasks: &mut Vec<TaskInfo>, filters: Vec<String>) -> Result<()> {
    let mut table = Table::new();

    table.set_format(
        format::FormatBuilder::new()
            .padding(1, 1)
            .separators(
                &[format::LinePosition::Title],
                format::LineSeparator::new('─', ' ', ' ', ' '),
            )
            .build(),
    );

    table.set_titles(row!["ID", "Title", "Project", "Assigned", "Due", "Rank"]);

    for filter in filters {
        apply_filter(tasks, filter);
    }

    tasks.sort_by(|a, b| b.rank.partial_cmp(&a.rank).unwrap());

    let mut min_rank = 0.0;
    let mut max_rank = 0.0;
    if tasks.first().is_some() {
        max_rank = tasks.first().unwrap().rank;
    }
    if tasks.last().is_some() {
        min_rank = tasks.last().unwrap().rank;
    }

    for task in tasks {
        let state = task.events.last().unwrap_or(&TaskEvent::default()).action.clone();

        let (max_style, min_style, mid_style, gen_style) = if state == "open" {
            ("bFC", "Fb", "Fc", "")
        } else {
            ("iFYBd", "iFYBd", "iFYBd", "iFYBd")
        };

        let rank = task.rank.to_string();

        table.add_row(Row::new(vec![
            Cell::new(&task.id.to_string()).style_spec(gen_style),
            Cell::new(&task.title).style_spec(gen_style),
            Cell::new(&task.project.join(", ")).style_spec(gen_style),
            Cell::new(&task.assign.join(", ")).style_spec(gen_style),
            Cell::new(&timestamp_to_date(task.due.unwrap_or(0), "date")).style_spec(gen_style),
            if task.rank == max_rank {
                Cell::new(&rank).style_spec(max_style)
            } else if task.rank == min_rank {
                Cell::new(&rank).style_spec(min_style)
            } else {
                Cell::new(&rank).style_spec(mid_style)
            },
        ]));
    }
    table.printstd();

    Ok(())
}

pub fn print_task_info(taskinfo: TaskInfo) -> Result<()> {
    let current_state = &taskinfo.events.last().unwrap_or(&TaskEvent::default()).action.clone();
    let due = timestamp_to_date(taskinfo.due.unwrap_or(0), "date");
    let created_at = timestamp_to_date(taskinfo.created_at, "datetime");
    let mut table = table!([Bd => "ref_id", &taskinfo.ref_id],
                           ["id", &taskinfo.id.to_string()],
                           [Bd =>"owner", &taskinfo.owner],
                           [Bd =>"title", &taskinfo.title],
                           ["desc", &taskinfo.desc.to_string()],
                           [Bd =>"assign", taskinfo.assign.join(", ")],
                           ["project", taskinfo.project.join(", ")],
                           [Bd =>"due", due],
                           ["rank", &taskinfo.rank.to_string()],
                           [Bd =>"created_at", created_at],
                           ["current_state", current_state]);

    table.set_format(
        format::FormatBuilder::new()
            .padding(1, 1)
            .separators(
                &[format::LinePosition::Title],
                format::LineSeparator::new('─', ' ', ' ', ' '),
            )
            .build(),
    );
    table.set_titles(row!["Name", "Value"]);

    table.printstd();

    let mut event_table = table!(["events", &events_as_string(taskinfo.events)]);
    event_table.set_format(*format::consts::FORMAT_NO_COLSEP);

    event_table.printstd();

    Ok(())
}

pub fn comments_as_string(comments: Vec<Comment>) -> String {
    let mut comments_str = String::new();
    for comment in comments {
        comments_str.push_str(&comment.to_string());
        comments_str.push('\n');
    }
    comments_str.pop();
    comments_str
}

pub fn events_as_string(events: Vec<TaskEvent>) -> String {
    let mut events_str = String::new();
    for event in events {
        events_str.push_str("State changed to ");
        events_str.push_str(&event.action.to_string());
        events_str.push_str(" at ");
        events_str.push_str(&event.timestamp.to_string());
        events_str.push('\n');
    }
    events_str
}
