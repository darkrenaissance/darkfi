// make async task that updates info
// this display that
//use drk::Result;
use std::{
    error::Error,
    io,
    io::Read,
    time::{Duration, Instant},
};
use termion::{async_stdin, event::Key, input::TermRead, raw::IntoRawMode};
use tui::{
    backend::{Backend, TermionBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Spans, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Terminal,
};

pub mod app;
pub mod list;
pub mod ui;

use crate::app::App;

fn main() -> Result<(), io::Error> {
    // Set up terminal output
    let stdout = io::stdout().into_raw_mode()?;
    let backend = TermionBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // create app and run it
    let tick_rate = Duration::from_millis(250);
    let app = App::new();
    let res = run_app(&mut terminal, app, tick_rate);
    
    terminal.clear()?;

    if let Err(err) = res {
        println!("{:?}", err)
    }

    Ok(())
    // Create a separate thread to poll stdin.
    // This provides non-blocking input support.
    //let mut asi = async_stdin();

    //// Clear the terminal before first draw.
    //terminal.clear()?;
    //loop {
    //    // Lock the terminal and start a drawing session.
    //    terminal.draw(|frame| {
    //        // Create a layout into which to place our blocks.
    //        let chunks = Layout::default()
    //            .direction(Direction::Vertical)
    //            .constraints([Constraint::Percentage(6), Constraint::Percentage(94)].as_ref())
    //            .split(frame.size());

    //        //let size = frame.size();

    //        // The text lines for our text box.
    //        let txt = vec![Spans::from("\n Press q to quit.\n")];
    //        // Create a paragraph with the above text...
    //        let graph = Paragraph::new(txt)
    //            // In a block with borders and the given title...
    //            .block(Block::default().title("").borders(Borders::ALL))
    //            // With white foreground and black background...
    //            .style(Style::default().fg(Color::White).bg(Color::Black));

    //        // Render into the layout.
    //        frame.render_widget(graph, chunks[0]);

    //        // create a list
    //        //let mut items: Vec<ListItem> = Vec::new();
    //        //for num in 1..100 {
    //        //    let new_item = ListItem::new(format!("Node {}", num));
    //        //    items.push(new_item);
    //        //}

    //        //let list = List::new(items)
    //        //    .block(Block::default().title("Nodes").borders(Borders::ALL))
    //        //    .style(Style::default().fg(Color::White))
    //        //    .highlight_style(Style::default().add_modifier(Modifier::ITALIC))
    //        //    .highlight_symbol(">>");

    //        //// draw a list
    //        //frame.render_widget(list, chunks[1]);

    //        // make a paragraph
    //        let mut text1 = String::new();
    //        for num in 1..10000 {
    //            let text2 = format!("\n Node {}\n", num);
    //            text1.push_str(&text2);
    //        }

    //        let text = Spans::from(vec![Span::raw(String::from(text1))]);
    //        let graph = Paragraph::new(text)
    //            .block(Block::default().title("").borders(Borders::ALL))
    //            .style(Style::default().fg(Color::White).bg(Color::Black))
    //            .scroll((0, 10000))
    //            .wrap(Wrap { trim: true });

    //        frame.render_widget(graph, chunks[1]);
    //    })?;

    //    // Iterate over all the keys that have been pressed since the
    //    // last time we checked.
    //    for k in asi.by_ref().keys() {
    //        match k.unwrap() {
    //            // If any of them is q, quit
    //            Key::Char('q') => {
    //                // Clear the terminal before exit so as not to leave
    //                // a mess.
    //                terminal.clear()?;
    //                return Ok(())
    //            }
    //            Key::Char('j') => {}
    //            // Otherwise, throw them away.
    //            _ => (),
    //        }
    //    }
    //}
}

fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    mut app: App,
    tick_rate: Duration,
) -> io::Result<()> {
    let mut asi = async_stdin();

    let mut last_tick = Instant::now();
    loop {
        terminal.draw(|f| ui::ui(f, &mut app))?;

        for k in asi.by_ref().keys() {
            match k.unwrap() {
                // If any of them is q, quit
                Key::Char('q') => {
                    // Clear the terminal before exit so as not to leave
                    // a mess.
                    terminal.clear()?;
                    return Ok(())
                }
                Key::Char('j') => app.items.next(),
                Key::Char('k') => app.items.previous(),
                // Otherwise, throw them away.
                _ => (),
            }
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }
}
