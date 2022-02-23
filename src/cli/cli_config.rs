use std::{
    env, fs,
    io::Write,
    marker::PhantomData,
    path::{Path, PathBuf},
    str,
};

use serde::{de::DeserializeOwned, Serialize};

use clap::ArgMatches;
use simplelog::ConfigBuilder;

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
        println!("Config file created in `{:?}`. Please review it and try again.", path);
        std::process::exit(2);
    }

    Ok(())
}

pub fn log_config(matches: ArgMatches) -> Result<(simplelog::LevelFilter, simplelog::Config)> {
    let mut verbosity_level = 0;
    verbosity_level += matches.occurrences_of("verbose");
    let log_level = match verbosity_level {
        0 => simplelog::LevelFilter::Info,
        1 => simplelog::LevelFilter::Debug,
        _ => simplelog::LevelFilter::Trace,
    };

    let log_config = match env::var("LOG_TARGETS") {
        Ok(x) => {
            let targets: Vec<&str> = x.split(',').collect();
            let mut cfgbuilder = ConfigBuilder::new();
            for i in targets {
                cfgbuilder.add_filter_allow(i.to_string());
            }

            cfgbuilder.build()
        }
        Err(_) => simplelog::Config::default(),
    };

    Ok((log_level, log_config))
}
