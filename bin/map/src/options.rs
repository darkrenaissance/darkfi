use clap::{App, Arg, ArgMatches};

use darkfi::Result;

pub struct ProgramOptions {
    pub log_path: Box<std::path::PathBuf>,
    pub app: ArgMatches,
}

impl ProgramOptions {
    pub fn load() -> Result<ProgramOptions> {
        let app = App::new("dfi")
            .version("0.1.0")
            .author("lunar_mining")
            .about("Map")
            .arg(
                Arg::new("LOG_PATH")
                    .long("log")
                    .value_name("LOG_PATH")
                    .help("Logfile path")
                    .takes_value(true),
            )
            .arg(
                Arg::new("verbose")
                    .short('v')
                    .long("verbose")
                    .multiple_occurrences(true)
                    .help("Sets the level of verbosity"),
            )
            .get_matches();

        let log_path = Box::new(
            if let Some(log_path) = app.value_of("LOG_PATH") {
                std::path::Path::new(log_path)
            } else {
                std::path::Path::new("/tmp/map.log")
            }
            .to_path_buf(),
        );

        Ok(ProgramOptions { log_path, app })
    }
}
