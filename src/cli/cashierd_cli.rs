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
                Arg::new("verbose")
                    .short('v')
                    .help_heading(Some("Increase verbosity"))
                    .long("verbose")
                    .takes_value(false),
            )
            .arg(
                Arg::new("hello")
                    .long("hello")
                    .help_heading(Some("Say hello"))
                    .takes_value(false),
            )
            .arg(
                Arg::new("wallet")
                    .short('w')
                    .long("wallet")
                    .help_heading(Some("Create a new wallet"))
                    .takes_value(false),
            )
            .arg(
                Arg::new("info")
                    .short('i')
                    .long("info")
                    .help_heading(Some("Request info from daemon"))
                    .takes_value(false),
            )
            .arg(
                Arg::new("stop")
                    .short('s')
                    .long("stop")
                    .help_heading(Some("Send a stop signal to the daemon"))
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
