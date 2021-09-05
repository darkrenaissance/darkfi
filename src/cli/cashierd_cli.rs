use crate::Result;
use clap::{App, Arg};

pub struct CashierdCli {
    pub verbose: bool,
    pub hello: bool,
    pub wallet: bool,
    pub info: bool,
    pub stop: bool,
}

impl CashierdCli {
    pub fn load() -> Result<Self> {
        let app = App::new("Cashier CLI")
            .version("0.1.0")
            .author("Dark Renaissance Technologies")
            .about("run service daemon")
            .arg(
                Arg::with_name("verbose")
                    .short("v")
                    .help("Increase verbosity")
                    .long("verbose")
                    .takes_value(false),
            )
            .arg(
                Arg::with_name("hello")
                    .long("hello")
                    .help("Say hello")
                    .takes_value(false),
            )
            .arg(
                Arg::with_name("wallet")
                    .short("w")
                    .long("wallet")
                    .help("Create a new wallet")
                    .takes_value(false),
            )
            .arg(
                Arg::with_name("info")
                    .short("i")
                    .long("info")
                    .help("Request info from daemon")
                    .takes_value(false),
            )
            .arg(
                Arg::with_name("stop")
                    .short("s")
                    .long("stop")
                    .help("Send a stop signal to the daemon")
                    .takes_value(false),
            )
            .get_matches();

        let verbose = app.is_present("verbose");
        let wallet = app.is_present("wallet");
        let info = app.is_present("info");
        let hello = app.is_present("hello");
        let stop = app.is_present("stop");

        Ok(Self {
            verbose,
            wallet,
            info,
            hello,
            stop,
        })
    }
}
