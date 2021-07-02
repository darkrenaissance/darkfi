
use std::net::SocketAddr;

use crate::Result;

use clap::{App, Arg};



pub struct WalletCli {
    pub connect_addr: Option<SocketAddr>,
    pub sub_addr: Option<SocketAddr>,
    pub verbose: bool,
    pub database_path: Box<std::path::PathBuf>,
    pub log_path: Box<std::path::PathBuf>,
    pub rpc_port: u16,
}

impl WalletCli {
    pub fn load() -> Result<Self> {
        // let app = App::new("dfi").version("0.1.0").author("Amir Taaki <amir@dyne.org>").about("Run Service Client");
        let app = clap_app!(dfi =>
            (version: "0.1.0")
            (author: "Amir Taaki <amir@dyne.org>")
            (about: "Run Service Client")
            (@arg CONNECT: -c --connect +takes_value "Connect add//ress")
            (@arg SUB_ADDR: -s --subaddr +takes_value "Subscriber addr")
            (@arg VERBOSE: -v --verbose "Increase verbosity")
            (@arg DATABASE_PATH: --database +takes_value "database path")
            (@arg LOG_PATH: --log +takes_value "Logfile path")
            (@arg RPC_PORT: -r --rpc +takes_value "RPC port")
        )
        .get_matches();

        let connect_addr = if let Some(connect_addr) = app.value_of("CONNECT") {
            Some(connect_addr.parse()?)
        } else {
            None
        };

        let sub_addr = if let Some(sub_addr) = app.value_of("SUB_ADDR") {
            Some(sub_addr.parse()?)
        } else {
            None
        };

        let verbose = app.is_present("VERBOSE");

        let database_path = Box::new(
            if let Some(database_path) = app.value_of("DATABASE_PATH") {
                std::path::Path::new(database_path)
            } else {
                std::path::Path::new("database_client.db")
            }
            .to_path_buf(),
        );

        let log_path = Box::new(
            if let Some(log_path) = app.value_of("LOG_PATH") {
                std::path::Path::new(log_path)
            } else {
                std::path::Path::new("/tmp/darkfid_service_daemon.log")
            }
            .to_path_buf(),
        );

        let rpc_port = if let Some(rpc_port) = app.value_of("RPC_PORT") {
            rpc_port.parse()?
        } else {
            8000
        };

        Ok(Self{
            connect_addr,
            sub_addr,
            verbose,
            database_path,
            log_path,
            rpc_port,
        })
    }
}
