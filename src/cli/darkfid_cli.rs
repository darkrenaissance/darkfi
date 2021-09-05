use crate::Result;

use std::path::PathBuf;

use clap::{App, Arg};

pub struct DarkfidCli {
    pub verbose: bool,
    pub config: Box<Option<PathBuf>>,
}

impl DarkfidCli {
    pub fn load() -> Result<Self> {
        let app = App::new("Darkfi Daemon CLI")
            .version("0.1.0")
            .author("Dark Renaissance Technologies")
            .about("Run Darkfi Daemon")
            .arg(
                Arg::with_name("verbose")
                    .short("v")
                    .help("Increase verbosity")
                    .long("verbose")
                    .takes_value(false),
            )
            .arg(
                Arg::with_name("config")
                    .short("c")
                    .help("Path for config file")
                    .long("config")
                    .takes_value(true),
            )
            .get_matches();

        let config = Box::new(if let Some(config_path) = app.value_of("config") {
            Some(std::path::Path::new(config_path).to_path_buf())
        } else {
            None
        });

        let verbose = app.is_present("verbose");

        Ok(Self { verbose, config })
    }
}
