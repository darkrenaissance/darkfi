use drk::Result;
use rand::{thread_rng, Rng};
use std::{io, io::Read, time::Instant};
use termion::{async_stdin, event::Key, input::TermRead, raw::IntoRawMode};
use tui::{
    backend::TermionBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::Spans,
    widgets::{Block, Borders, List, ListItem, Paragraph, Widget},
    Terminal,
};

fn main() -> Result<()> {
    // Set up terminal output
    let stdout = io::stdout().into_raw_mode()?;
    let backend = TermionBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create a separate thread to poll stdin.
    // This provides non-blocking input support.
    let mut asi = async_stdin();

    // Clear the terminal before first draw.
    terminal.clear()?;
    loop {
        // Lock the terminal and start a drawing session.
        terminal.draw(|frame| {
            // Create a layout into which to place our blocks.
            let size = frame.size();

            // The text lines for our text box.
            let txt = vec![Spans::from("\n Press q to quit.\n")];
            // Create a paragraph with the above text...
            let graph = Paragraph::new(txt)
                // In a block with borders and the given title...
                .block(Block::default().title("List of active nodes").borders(Borders::ALL))
                // With white foreground and black background...
                .style(Style::default().fg(Color::White).bg(Color::Black));

            // Render into the layout.
            frame.render_widget(graph, size);

            // create a list
            let mut items: Vec<ListItem> = Vec::new();
            for num in 1..11 {
                let new_item = ListItem::new(format!("Node {}", num));
                items.push(new_item);
            }

            let list = List::new(items)
                .block(Block::default().title("Nodes").borders(Borders::ALL))
                .style(Style::default().fg(Color::White))
                .highlight_style(Style::default().add_modifier(Modifier::ITALIC))
                .highlight_symbol(">>");

            // draw a list
            frame.render_widget(list, size);
        })?;

        // Iterate over all the keys that have been pressed since the
        // last time we checked.
        for k in asi.by_ref().keys() {
            match k.unwrap() {
                // If any of them is q, quit
                Key::Char('q') => {
                    // Clear the terminal before exit so as not to leave
                    // a mess.
                    terminal.clear()?;
                    return Ok(())
                }
                // Otherwise, throw them away.
                _ => (),
            }
        }
    }
}
