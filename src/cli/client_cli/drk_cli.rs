use super::cli_config::DrkCliConfig;
use crate::Result;

use clap::{App, AppSettings, Arg};

pub struct DrkCli {
    pub change_config: bool,
    pub verbose: bool,
}

impl DrkCli {
    pub fn load(config: &mut DrkCliConfig) -> Result<Self> {
        let app = App::new("Drk CLI")
            .version("0.1.0")
            .author("Amir Taaki <amir@dyne.org>")
            .about("Run Drk Client")
            .arg(
                Arg::new("verbose")
                    .short('v')
                    .help_heading(Some("Increase verbosity"))
                    .long("verbose")
                    .takes_value(false),
            )
            .subcommand(
                App::new("config")
                    .about("Configuration settings")
                    .aliases(&["get", "set"])
                    .setting(AppSettings::SubcommandRequiredElseHelp)
                    .subcommand(App::new("get").about("Get configuration settings"))
                    .subcommand(
                        App::new("set")
                            .about("Set configuration settings")
                            .args(&[
                                Arg::new("rpc_url")
                                    .about("Set RPC Url")
                                    .long("rpc-url")
                                    .takes_value(true),
                                Arg::new("log_path")
                                    .about("Set Log Path")
                                    .long("log-path")
                                    .takes_value(true),
                            ])
                            .setting(AppSettings::ArgRequiredElseHelp),
                    ),
            )
            .get_matches();

        let mut change_config = false;

        let verbose = app.is_present("verbose");

        match app.subcommand_matches("config") {
            Some(config_sub) => match config_sub.subcommand() {
                Some(c) => match c {
                    ("get", _) => {
                        change_config = true;
                        println!("RPC Url: {}", config.rpc_url);
                        println!("Log Path: {}", config.log_path);
                    }
                    ("set", c) => {
                        change_config = true;
                        if let Some(v) = c.value_of("rpc_url") {
                            config.rpc_url = v.to_string();
                            println!("Change RPC Url To {}", config.rpc_url);
                        }
                        if let Some(v) = c.value_of("log_path") {
                            config.log_path = v.to_string();
                            println!("Change Log Path To {}", config.log_path);
                        }
                    }
                    _ => {}
                },
                None => {}
            },
            None => {}
        }

        Ok(Self {
            change_config,
            verbose,
        })
    }
}
