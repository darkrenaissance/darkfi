use crate::Result;
use std::net::SocketAddr;

pub struct ProgramOptions {
    pub accept_addr: Option<SocketAddr>,
    pub pub_addr: Option<SocketAddr>,
    pub verbose: bool,
    pub log_path: Box<std::path::PathBuf>,
}


impl ProgramOptions {
    pub fn load() -> Result<ProgramOptions> {
        let app = clap_app!(dfi =>
            (version: "0.1.0")
            (author: "Amir Taaki <amir@dyne.org>")
            (about: "run service daemon")
            (@arg ACCEPT: -a --accept +takes_value "Accept add//ress")
            (@arg PUB_ADDR: -p --pubaddr +takes_value "Publisher addr")
            (@arg VERBOSE: -v --verbose "Increase verbosity")
            (@arg LOG_PATH: --log +takes_value "Logfile path")
        )
            .get_matches();

        let accept_addr = if let Some(accept_addr) = app.value_of("ACCEPT") {
            Some(accept_addr.parse()?)
        } else {
            None
        };

        let pub_addr = if let Some(pub_addr) = app.value_of("PUB_ADDR") {
            Some(pub_addr.parse()?)
        } else {
            None
        };

        let verbose = app.is_present("VERBOSE");

        let log_path = Box::new(
            if let Some(log_path) = app.value_of("LOG_PATH") {
                std::path::Path::new(log_path)
            } else {
                std::path::Path::new("/tmp/darkfid_service_daemon.log")
            }
            .to_path_buf(),
        );

        Ok(ProgramOptions {
            accept_addr,
            pub_addr,
            verbose,
            log_path
        })
    }
}
