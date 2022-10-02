use std::{
    env, fs,
    io::Write,
    marker::PhantomData,
    path::{Path, PathBuf},
    str,
    time::Duration,
};

use indicatif::{ProgressBar, ProgressStyle};
use serde::{de::DeserializeOwned, Serialize};
use simplelog::ConfigBuilder;
use termion::color;

use crate::{Error, Result};

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

pub fn spawn_config(path: &Path, contents: &[u8]) -> Result<()> {
    if !path.exists() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut file = fs::File::create(path)?;
        file.write_all(contents)?;
        println!("Config file created in '{:?}'. Please review it and try again.", path);
        std::process::exit(2);
    }

    Ok(())
}

pub fn get_log_level(verbosity_level: u64) -> simplelog::LevelFilter {
    match verbosity_level {
        0 => simplelog::LevelFilter::Info,
        1 => simplelog::LevelFilter::Debug,
        _ => simplelog::LevelFilter::Trace,
    }
}

pub fn get_log_config() -> simplelog::Config {
    match env::var("LOG_TARGETS") {
        Ok(x) => {
            let targets: Vec<String> = x.split(',').map(|x| x.to_string()).collect();
            let mut cfgbuilder = ConfigBuilder::new();

            for i in targets {
                if i.starts_with('!') {
                    cfgbuilder.add_filter_ignore(i.trim_start_matches('!').to_string());
                } else {
                    cfgbuilder.add_filter_allow(i);
                }
            }

            cfgbuilder.build()
        }
        Err(_) => simplelog::Config::default(),
    }
}

/// This macro is used for a standard way of daemonizing darkfi binaries
/// with TOML config file configuration, and argument parsing. It also
/// spawns a multithreaded async executor and passes it into the given
/// function.
///
/// The Cargo.toml dependencies needed for this are:
/// ```text
/// async-channel = "1.6.1"
/// async-executor = "1.4.1"
/// async-std = "1.11.0"
/// darkfi = { path = "../../", features = ["util"] }
/// easy-parallel = "3.2.0"
/// futures-lite = "1.12.0"
/// simplelog = "0.12.0-alpha1"
///
/// # Argument parsing
/// serde = "1.0.136"
/// serde_derive = "1.0.136"
/// structopt = "0.3.26"
/// structopt-toml = "0.5.0"
/// ```
///
/// Example usage:
/// ```text
/// use async_std::sync::Arc;
/// use futures_lite::future;
/// use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};
///
/// use darkfi::{
///     async_daemonize, cli_desc,
///     util::{
///         cli::{get_log_config, get_log_level, spawn_config},
///         path::get_config_path, expand_path
///     },
///     Result,
/// };
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
///     #[structopt(short, parse(from_occurrences))]
///     /// Increase verbosity (-vvv supported)
///     verbose: u8,
/// }
///
/// async_daemonize!(realmain);
/// async fn realmain(args: Args, ex: Arc<Executor<'_>>) -> Result<()> {
///     println!("Hello, world!");
///     Ok(())
/// }
/// ```
#[cfg(feature = "async-runtime")]
#[macro_export]
macro_rules! async_daemonize {
    ($realmain:ident) => {
        fn main() -> Result<()> {
            let args = Args::from_args_with_toml("").unwrap();
            let cfg_path = get_config_path(args.config, CONFIG_FILE)?;
            spawn_config(&cfg_path, CONFIG_FILE_CONTENTS.as_bytes())?;
            let args = Args::from_args_with_toml(&std::fs::read_to_string(cfg_path)?).unwrap();

            let log_level = get_log_level(args.verbose.into());
            let log_config = get_log_config();

            let log_file_path = match std::env::var("DARKFI_LOG") {
                Ok(p) => p,
                Err(_) => {
                    let bin_name = if let Some(bin_name) = option_env!("CARGO_BIN_NAME") {
                        bin_name
                    } else {
                        "darkfi"
                    };
                    std::fs::create_dir_all(expand_path("~/.local/darkfi")?)?;
                    format!("~/.local/darkfi/{}.log", bin_name)
                }
            };

            let log_file_path = expand_path(&log_file_path)?;
            let log_file = std::fs::File::create(log_file_path)?;

            simplelog::CombinedLogger::init(vec![
                simplelog::TermLogger::new(
                    log_level,
                    log_config.clone(),
                    simplelog::TerminalMode::Mixed,
                    simplelog::ColorChoice::Auto,
                ),
                simplelog::WriteLogger::new(log_level, log_config, log_file),
            ])?;

            // https://docs.rs/smol/latest/smol/struct.Executor.html#examples
            let ex = Arc::new(async_executor::Executor::new());
            let (signal, shutdown) = async_channel::unbounded::<()>();
            let (_, result) = easy_parallel::Parallel::new()
                // Run four executor threads
                .each(0..4, |_| future::block_on(ex.run(shutdown.recv())))
                // Run the main future on the current thread.
                .finish(|| {
                    future::block_on(async {
                        $realmain(args, ex.clone()).await?;
                        drop(signal);
                        Ok::<(), darkfi::Error>(())
                    })
                });

            result
        }
    };
}

pub fn progress_bar(message: &str) -> ProgressBar {
    let progress_bar = ProgressBar::new(42);
    progress_bar.set_style(
        ProgressStyle::default_spinner().template("{spinner:.green} {wide_msg}").unwrap(),
    );
    progress_bar.enable_steady_tick(Duration::from_millis(100));
    progress_bar.set_message(message.to_string());
    progress_bar
}

pub fn fg_red(message: &str) -> String {
    format!("{}{}{}", color::Fg(color::Red), message, color::Fg(color::Reset))
}

pub fn fg_green(message: &str) -> String {
    format!("{}{}{}", color::Fg(color::Green), message, color::Fg(color::Reset))
}
