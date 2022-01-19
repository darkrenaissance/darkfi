// select each connection and show log of traffic
// use rpc to get some info from the ircd network
// ircd::logger keeps track of network info
// map rpc polls logger for info about nodes, etc
use darkfi::{
    error::{Error, Result},
    rpc::{jsonrpc, jsonrpc::JsonResult},
};

use log::{debug, error};
use serde_json::{json, Value};
use std::{io, io::Read, time::Duration};
use termion::{async_stdin, event::Key, input::TermRead, raw::IntoRawMode};
use tui::{
    backend::{Backend, TermionBackend},
    Terminal,
};

use map::{ui, App};

struct Map {
    url: String,
}

impl Map {
    pub fn new(url: String) -> Self {
        Self { url }
    }

    async fn request(&self, r: jsonrpc::JsonRequest) -> Result<Value> {
        let reply: JsonResult = match jsonrpc::send_request(&self.url, json!(r)).await {
            Ok(v) => v,
            Err(e) => return Err(e),
        };

        match reply {
            JsonResult::Resp(r) => {
                debug!(target: "RPC", "<-- {}", serde_json::to_string(&r)?);
                Ok(r.result)
            }

            JsonResult::Err(e) => {
                debug!(target: "RPC", "<-- {}", serde_json::to_string(&e)?);
                Err(Error::JsonRpcError(e.error.message.to_string()))
            }

            JsonResult::Notif(n) => {
                debug!(target: "RPC", "<-- {}", serde_json::to_string(&n)?);
                Err(Error::JsonRpcError("Unexpected reply".to_string()))
            }
        }
    }
}

fn main() -> Result<()> {
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
    _tick_rate: Duration,
) -> io::Result<()> {
    let mut asi = async_stdin();

    terminal.clear()?;

    app.node_list.state.select(Some(0));

    app.node_info.index = 0;
    //let mut last_tick = Instant::now();

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
