// next/ prev:
//      select_node(i)
//      NodeInfo
//          set-content(node_info)
//              clear current text
//              let text = ...

use std::{
    io,
    io::Read,
    time::{Duration, Instant},
};
use termion::{async_stdin, event::Key, input::TermRead, raw::IntoRawMode};
use tui::{
    backend::{Backend, TermionBackend},
    Terminal,
};

pub mod app;
pub mod node;
pub mod list;
pub mod types;
pub mod ui;

use crate::app::App;

fn main() -> Result<(), io::Error> {
    // Set up terminal output
    let stdout = io::stdout().into_raw_mode()?;
    let backend = TermionBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // create app and run it
    let tick_rate = Duration::from_millis(250);
    // here
    let app = App::new();
    let res = run_app(&mut terminal, app, tick_rate);

    terminal.clear()?;

    if let Err(err) = res {
        println!("{:?}", err)
    }

    Ok(())
}

fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    mut app: App,
    tick_rate: Duration,
) -> io::Result<()> {
    let mut asi = async_stdin();

    terminal.clear()?;

    app.node_list.state.select(Some(0));

    let mut last_tick = Instant::now();

    loop {
        terminal.draw(|f| ui::ui(f, &mut app))?;

        for k in asi.by_ref().keys() {
            match k.unwrap() {
                Key::Char('q') => {
                    terminal.clear()?;
                    return Ok(())
                }
                Key::Char('j') => {
                    app.node_list.next();
                    app.node_info.next();
                }
                Key::Char('k') => {
                    app.node_list.previous();
                    app.node_info.previous();
                }
                _ => (),
            }
        }

        //if last_tick.elapsed() >= tick_rate {
        //    app.clone().update();
        //    last_tick = Instant::now();
        //}
    }
}
