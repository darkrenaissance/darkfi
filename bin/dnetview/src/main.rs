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

use std::{fs::File, io, io::Read};

use async_std::sync::Arc;
use clap::Parser;
use easy_parallel::Parallel;
use log::info;
use simplelog::*;
use smol::Executor;
use termion::{async_stdin, event::Key, input::TermRead, raw::IntoRawMode};
use tui::{
    backend::{Backend, TermionBackend},
    Terminal,
};

use darkfi::util::{
    async_util,
    cli::{get_log_config, get_log_level, spawn_config, Config},
    path::{expand_path, get_config_path},
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
    config::{DnvConfig, CONFIG_FILE, CONFIG_FILE_CONTENTS},
    error::{DnetViewError, DnetViewResult},
    model::Model,
    options::Args,
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

            if let Some(e) = err {
                return Err(e)
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
            async_util::msleep(100).await;
        }
    }
}

#[async_std::main]
async fn main() -> DnetViewResult<()> {
    //debug!(target: "dnetview", "main() START");
    let args = Args::parse();

    let log_level = get_log_level(args.verbose.into());
    let log_config = get_log_config();

    let log_file_path = expand_path(&args.log_path)?;
    if let Some(parent) = log_file_path.parent() {
        std::fs::create_dir_all(parent)?;
    };

    let file = File::create(log_file_path)?;
    WriteLogger::init(log_level, log_config, file)?;
    info!("Log level: {}", log_level);

    let config_path = get_config_path(args.config, CONFIG_FILE)?;
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

    let mut dnetview = DnetView::new(model.clone(), view);
    let parser = DataParser::new(model, config);

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
