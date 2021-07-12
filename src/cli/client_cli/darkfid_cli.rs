//use super::cli_config::DarkfidCliConfig;
use crate::Result;

use clap::{App, Arg};

pub struct DarkfidCli {
    //pub change_config: bool,
    pub verbose: bool,
}

impl DarkfidCli {
    pub fn load() -> Result<Self> {
        let app = App::new("Darkfi Daemon CLI")
            .version("0.1.0")
            .author("Amir Taaki <amir@dyne.org>")
            .about("Run Darkfi Daemon")
            .arg(
                Arg::new("verbose")
                    .short('v')
                    .help_heading(Some("Increase verbosity"))
                    .long("verbose")
                    .takes_value(false),
            )
            //.subcommand(
            //    App::new("config")
            //        .about("Configuration settings")
            //        .aliases(&["get", "set"])
            //        .setting(AppSettings::SubcommandRequiredElseHelp)
            //        .subcommand(App::new("get").about("Get configuration settings"))
            //        .subcommand(
            //            App::new("set")
            //                .about("Set configuration settings")
            //                .args(&[
            //                    Arg::new("connect_url")
            //                        .about("Set Connect Url")
            //                        .long("connect-url")
            //                        .takes_value(true),
            //                    Arg::new("subscriber_url")
            //                        .about("Set Subscriber Url")
            //                        .long("subscriber-url")
            //                        .takes_value(true),
            //                    Arg::new("rpc_url")
            //                        .about("Set RPC Url")
            //                        .long("rpc-url")
            //                        .takes_value(true),
            //                    Arg::new("database_path")
            //                        .about("Set Database Path")
            //                        .long("database-path")
            //                        .takes_value(true),
            //                    Arg::new("log_path")
            //                        .about("Set Log Path")
            //                        .long("log-path")
            //                        .takes_value(true),
            //                    Arg::new("password")
            //                        .about("Set password")
            //                        .long("password")
            //                        .takes_value(true),
            //                ])
            //                .setting(AppSettings::ArgRequiredElseHelp),
            //        ),
            //)
            .get_matches();

        //let mut change_config = false;

        let verbose = app.is_present("verbose");

        //match app.subcommand_matches("config") {
        //    Some(config_sub) => match config_sub.subcommand() {
        //        Some(c) => match c {
        //            ("get", _) => {
        //                change_config = true;
        //                println!("Connect Url: {}", config.connect_url);
        //                println!("Subscriber Url: {}", config.subscriber_url);
        //                println!("RPC Url: {}", config.rpc_url);
        //                println!("Database path: {}", config.database_path);
        //                println!("Log Path: {}", config.log_path);
        //                if config.password.trim().is_empty() {
        //                    println!("Password is empty. Please set a password.")
        //                };
        //            }
        //            ("set", c) => {
        //                change_config = true;
        //                if let Some(v) = c.value_of("connect_url") {
        //                    config.connect_url = v.to_string();
        //                    println!("Change Connect Url To {}", config.connect_url);
        //                }
        //                if let Some(v) = c.value_of("subscriber_url") {
        //                    config.subscriber_url = v.to_string();
        //                    println!("Change Subscriber Url To {}", config.subscriber_url);
        //                }
        //                if let Some(v) = c.value_of("rpc_url") {
        //                    config.rpc_url = v.to_string();
        //                    println!("Change RPC Url To {}", config.rpc_url);
        //                }
        //                if let Some(v) = c.value_of("database_path") {
        //                    config.database_path = v.to_string();
        //                    println!("Change Database Path To {}", config.database_path);
        //                }
        //                if let Some(v) = c.value_of("log_path") {
        //                    config.log_path = v.to_string();
        //                    println!("Change Log Path To {}", config.log_path);
        //                }
        //                if let Some(v) = c.value_of("password") {
        //                    config.password = v.to_string();
        //                    println!("Updated password. New password: {}", config.password);
        //                }
        //            }
        //            _ => {}
        //        },
        //        None => {}
        //    },
        //    None => {}
        //}

        Ok(Self {
            //change_config,
            verbose,
        })
    }
}
