/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use std::{
    fs,
    io::Write,
    path::Path,
    str,
    sync::{Arc, Mutex},
    time::Instant,
};

use crate::Result;

/*
#[derive(Clone, Default)]
pub struct Config<T> {
    config: PhantomData<T>,
}

impl<T: Serialize + DeserializeOwned> Config<T> {
    pub fn load(path: PathBuf) -> Result<T> {
        if Path::new(&path).exists() {
            let toml = fs::read(&path)?;
            let str_buff = str::from_utf8(&toml)?;
            let config: T = toml::from_str(str_buff)?;
            Ok(config)
        } else {
            let path = path.to_str();
            if path.is_some() {
                println!("Could not find/parse configuration file in: {}", path.unwrap());
            } else {
                println!("Could not find/parse configuration file");
            }
            println!("Please follow the instructions in the README");
            Err(Error::ConfigNotFound)
        }
    }
}
*/

pub fn spawn_config(path: &Path, contents: &[u8]) -> Result<()> {
    if !path.exists() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut file = fs::File::create(path)?;
        file.write_all(contents)?;
        println!("Config file created in {path:?}. Please review it and try again.");
        std::process::exit(2);
    }

    Ok(())
}

/// This macro is used for a standard way of daemonizing darkfi binaries
/// with TOML config file configuration, and argument parsing.
///
/// It also spawns a multithreaded async executor and passes it into the
/// given function.
///
/// The Cargo.toml dependencies needed for this are:
/// ```text
/// darkfi = { path = "../../", features = ["util"] }
/// easy-parallel = "3.2.0"
/// signal-hook-async-std = "0.2.2"
/// signal-hook = "0.3.15"
/// tracing-subscriber = "0.3.19"
/// tracing-appender = "0.2.3"
/// smol = "1.2.5"
///
/// # Argument parsing
/// serde = {version = "1.0.135", features = ["derive"]}
/// structopt = "0.3.26"
/// structopt-toml = "0.5.1"
/// ```
///
/// Example usage:
/// ```
/// use darkfi::{async_daemonize, cli_desc, Result};
/// use smol::stream::StreamExt;
/// use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};
///
/// const CONFIG_FILE: &str = "daemond_config.toml";
/// const CONFIG_FILE_CONTENTS: &str = include_str!("../daemond_config.toml");
///
/// #[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
/// #[serde(default)]
/// #[structopt(name = "daemond", about = cli_desc!())]
/// struct Args {
///     #[structopt(short, long)]
///     /// Configuration file to use
///     config: Option<String>,
///
///     #[structopt(short, long)]
///     /// Set log file to ouput into
///     log: Option<String>,
///
///     #[structopt(short, parse(from_occurrences))]
///     /// Increase verbosity (-vvv supported)
///     verbose: u8,
/// }
///
/// async_daemonize!(realmain);
/// async fn realmain(args: Args, ex: Arc<smol::Executor<'static>>) -> Result<()> {
///     println!("Hello, world!");
///     Ok(())
/// }
/// ```
#[cfg(feature = "async-daemonize")]
#[macro_export]
macro_rules! async_daemonize {
    ($realmain:ident) => {
        fn main() -> Result<()> {
            let args = match Args::from_args_with_toml("") {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Unable to get args: {e}");
                    return Err(Error::ConfigInvalid)
                }
            };
            let cfg_path =
                match darkfi::util::path::get_config_path(args.config.clone(), CONFIG_FILE) {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!("Unable to get config path `{:?}`: {e}", args.config);
                        return Err(e)
                    }
                };
            if let Err(e) =
                darkfi::util::cli::spawn_config(&cfg_path, CONFIG_FILE_CONTENTS.as_bytes())
            {
                eprintln!("Spawn config failed `{cfg_path:?}`: {e}");
                return Err(e)
            }
            let cfg_text = match std::fs::read_to_string(&cfg_path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Read config failed `{cfg_path:?}`: {e}");
                    return Err(e.into())
                }
            };
            let args = match Args::from_args_with_toml(&cfg_text) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Parsing config failed `{cfg_path:?}`: {e}");
                    return Err(Error::ConfigInvalid)
                }
            };

            // If a log file has been configured, create a terminal and file logger.
            // Otherwise, output to terminal logger only.
            let (non_blocking, file_guard) = match args.log {
                Some(ref log_path) => {
                    let log_path = match darkfi::util::path::expand_path(log_path) {
                        Ok(v) => v,
                        Err(e) => {
                            eprintln!("Expanding log path failed `{log_path:?}`: {e}");
                            return Err(e)
                        }
                    };
                    let log_file = match std::fs::File::create(&log_path) {
                        Ok(v) => v,
                        Err(e) => {
                            eprintln!("Creating log file failed `{log_path:?}`: {e}");
                            return Err(e.into())
                        }
                    };

                    // Hold guard until process stops to ensure buffer logs are flushed to file
                    let (non_blocking, guard) = tracing_appender::non_blocking(log_file);
                    (Some(non_blocking), Some(guard))
                }
                None => (None, None),
            };
            if let Err(e) = darkfi::util::logger::setup_logging(args.verbose, non_blocking) {
                if args.log.is_some() {
                    eprintln!("Unable to init logger with term + logfile combo: {e}");
                } else {
                    eprintln!("Unable to init term logger: {e}");
                }
                return Err(e.into())
            }

            // https://docs.rs/smol/latest/smol/struct.Executor.html#examples
            let n_threads = std::thread::available_parallelism().unwrap().get();
            let ex = std::sync::Arc::new(smol::Executor::new());
            let (signal, shutdown) = smol::channel::unbounded::<()>();
            let (_, result) = easy_parallel::Parallel::new()
                // Run four executor threads
                .each(0..n_threads, |_| smol::future::block_on(ex.run(shutdown.recv())))
                // Run the main future on the current thread.
                .finish(|| {
                    smol::future::block_on(async {
                        $realmain(args, ex.clone()).await?;
                        drop(signal);
                        Ok::<(), darkfi::Error>(())
                    })
                });

            result
        }

        /// Auxiliary structure used to keep track of signals
        struct SignalHandler {
            /// Termination signal channel receiver
            term_rx: smol::channel::Receiver<()>,
            /// Signals handle
            handle: signal_hook_async_std::Handle,
            /// SIGHUP publisher to retrieve new configuration,
            sighup_pub: darkfi::system::PublisherPtr<Args>,
        }

        impl SignalHandler {
            fn new(
                ex: std::sync::Arc<smol::Executor<'static>>,
            ) -> Result<(Self, smol::Task<Result<()>>)> {
                let (term_tx, term_rx) = smol::channel::bounded::<()>(1);
                let signals = signal_hook_async_std::Signals::new([
                    signal_hook::consts::SIGHUP,
                    signal_hook::consts::SIGTERM,
                    signal_hook::consts::SIGINT,
                    signal_hook::consts::SIGQUIT,
                ])?;
                let handle = signals.handle();
                let sighup_pub = darkfi::system::Publisher::new();
                let signals_task =
                    ex.spawn(handle_signals(signals, term_tx, sighup_pub.clone(), ex.clone()));

                Ok((Self { term_rx, handle, sighup_pub }, signals_task))
            }

            /// Handler waits for termination signal
            async fn wait_termination(&self, signals_task: smol::Task<Result<()>>) -> Result<()> {
                self.term_rx.recv().await?;
                print!("\r");
                self.handle.close();
                signals_task.await?;

                Ok(())
            }
        }

        /// Auxiliary task to handle SIGINT for forceful process abort
        async fn handle_abort(mut signals: signal_hook_async_std::Signals) {
            let mut n_sigint = 0;
            while let Some(signal) = signals.next().await {
                n_sigint += 1;
                if n_sigint == 2 {
                    print!("\r");
                    info!("Aborting. Good luck.");
                    std::process::abort();
                }
            }
        }

        /// Auxiliary task to handle SIGHUP, SIGTERM, SIGINT and SIGQUIT signals
        async fn handle_signals(
            mut signals: signal_hook_async_std::Signals,
            term_tx: smol::channel::Sender<()>,
            publisher: darkfi::system::PublisherPtr<Args>,
            ex: std::sync::Arc<smol::Executor<'static>>,
        ) -> Result<()> {
            while let Some(signal) = signals.next().await {
                match signal {
                    signal_hook::consts::SIGHUP => {
                        let args = Args::from_args_with_toml("").unwrap();
                        let cfg_path =
                            darkfi::util::path::get_config_path(args.config, CONFIG_FILE)?;
                        darkfi::util::cli::spawn_config(
                            &cfg_path,
                            CONFIG_FILE_CONTENTS.as_bytes(),
                        )?;
                        let args = Args::from_args_with_toml(&std::fs::read_to_string(cfg_path)?);
                        if args.is_err() {
                            println!("handle_signals():: Error parsing the config file");
                            continue
                        }
                        publisher.notify(args.unwrap()).await;
                    }
                    signal_hook::consts::SIGINT => {
                        // Spawn a new background task to listen for more SIGINT.
                        // This lets us forcefully abort the process if necessary.
                        let signals =
                            signal_hook_async_std::Signals::new([signal_hook::consts::SIGINT])?;
                        let handle = signals.handle();
                        ex.spawn(handle_abort(signals)).detach();

                        term_tx.send(()).await?;
                    }
                    signal_hook::consts::SIGTERM | signal_hook::consts::SIGQUIT => {
                        term_tx.send(()).await?;
                    }

                    _ => println!("handle_signals():: Unsupported signal"),
                }
            }
            Ok(())
        }
    };
}

pub fn fg_red(message: &str) -> String {
    format!("\x1b[31m{message}\x1b[0m")
}

pub fn fg_green(message: &str) -> String {
    format!("\x1b[32m{message}\x1b[0m")
}

pub fn fg_reset() -> String {
    "\x1b[0m".to_string()
}

pub struct ProgressInc {
    position: Arc<Mutex<u64>>,
    timer: Arc<Mutex<Option<Instant>>>,
}

impl Default for ProgressInc {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressInc {
    pub fn new() -> Self {
        eprint!("\x1b[?25l");
        Self { position: Arc::new(Mutex::new(0)), timer: Arc::new(Mutex::new(None)) }
    }

    pub fn inc(&self, n: u64) {
        let mut position = self.position.lock().unwrap();

        if *position == 0 {
            *self.timer.lock().unwrap() = Some(Instant::now());
        }

        *position += n;

        let binding = self.timer.lock().unwrap();
        let Some(elapsed) = binding.as_ref() else { return };
        let elapsed = elapsed.elapsed();
        let pos = *position;

        eprint!("\r[{elapsed:?}] {pos} attempts");
    }

    pub fn position(&self) -> u64 {
        *self.position.lock().unwrap()
    }

    pub fn finish_and_clear(&self) {
        *self.timer.lock().unwrap() = None;
        eprint!("\r\x1b[2K\x1b[?25h");
    }
}
