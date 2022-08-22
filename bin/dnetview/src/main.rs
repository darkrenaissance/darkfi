use async_std::sync::Arc;
use std::{fs::File, io, io::Read, path::PathBuf};

use darkfi::util::{
    cli::{get_log_config, get_log_level, spawn_config, Config},
    join_config_path,
};
use easy_parallel::Parallel;
use log::info;
use simplelog::*;
use smol::Executor;
use termion::{async_stdin, event::Key, input::TermRead, raw::IntoRawMode};
use tui::{
    backend::{Backend, TermionBackend},
    Terminal,
};
pub mod config;
pub mod error;
pub mod model;
pub mod options;
pub mod parser;
pub mod rpc;
pub mod util;
pub mod view;

use crate::{
    config::{DnvConfig, CONFIG_FILE_CONTENTS},
    error::{DnetViewError, DnetViewResult},
    model::Model,
    options::ProgramOptions,
    parser::DataParser,
    view::View,
};

struct DnetView {
    model: Arc<Model>,
    view: View,
}

impl DnetView {
    fn new(model: Arc<Model>, view: View) -> Self {
        Self { model, view }
    }

    async fn render_view<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> DnetViewResult<()> {
        let mut asi = async_stdin();

        terminal.clear()?;

        self.view.id_menu.state.select(Some(0));
        self.view.msg_list.state.select(Some(0));

        loop {
            self.view.update(
                self.model.msg_map.lock().await.clone(),
                self.model.selectables.lock().await.clone(),
                //self.model.selectables2.lock().await.clone(),
            );

            //debug!(target: "dnetview::render_view()", "ID MENU: {:?}", self.view.id_menu.ids);
            //debug!(target: "dnetview::render_view()", "SELECTABLES ID LIST: {:?}", self.model.selectables.lock().await.keys());

            let mut err: Option<DnetViewError> = None;

            terminal.draw(|f| match self.view.render(f) {
                Ok(()) => {}
                Err(e) => {
                    err = Some(e);
                }
            })?;

            match err {
                Some(e) => return Err(e),
                None => {}
            }

            self.view.msg_list.scroll()?;

            for k in asi.by_ref().keys() {
                match k.unwrap() {
                    Key::Char('q') => {
                        terminal.clear()?;
                        return Ok(())
                    }
                    Key::Char('j') => {
                        self.view.id_menu.next();
                    }
                    Key::Char('k') => {
                        self.view.id_menu.previous();
                    }
                    Key::Char('u') => {
                        // TODO
                        //view.msg_list.next();
                    }
                    Key::Char('d') => {
                        // TODO
                        //view.msg_list.previous();
                    }
                    _ => (),
                }
            }
            util::sleep(100).await;
        }
    }
}

#[async_std::main]
async fn main() -> DnetViewResult<()> {
    //debug!(target: "dnetview", "main() START");
    let options = ProgramOptions::load()?;

    let verbosity_level = options.app.occurrences_of("verbose");

    let log_level = get_log_level(verbosity_level);
    let log_config = get_log_config();

    let file = File::create(&*options.log_path).unwrap();
    WriteLogger::init(log_level, log_config, file)?;
    info!("Log level: {}", log_level);

    let config_path = join_config_path(&PathBuf::from("dnetview_config.toml"))?;

    spawn_config(&config_path, CONFIG_FILE_CONTENTS)?;

    let config = Config::<DnvConfig>::load(config_path)?;

    let stdout = io::stdout().into_raw_mode()?;
    let backend = TermionBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    terminal.clear()?;

    let model = Model::new();
    let view = View::new();

    let ex = Arc::new(Executor::new());
    let ex2 = ex.clone();

    let mut dnetview = DnetView::new(model.clone(), view.clone());
    let parser = DataParser::new(model.clone(), config);

    let nthreads = num_cpus::get();
    let (signal, shutdown) = async_channel::unbounded::<()>();

    let (_, result) = Parallel::new()
        .each(0..nthreads, |_| smol::future::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::future::block_on(async move {
                parser.start_connect_slots(ex2).await?;
                dnetview.render_view(&mut terminal).await?;
                drop(signal);
                Ok(())
            })
        });

    result
}
