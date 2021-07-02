use crate::Result;

use clap::{App, Arg};

pub struct WalletCli {
    pub verbose: bool,
}

impl WalletCli {
    pub fn load() -> Result<Self> {
        let app = App::new("Wallet CLI")
            .version("0.1.0")
            .author("Amir Taaki <amir@dyne.org>")
            .about("Run Service Client")
            .arg(
                Arg::new("verbose")
                    .short('v')
                    .help_heading(Some("Increase verbosity"))
                    .long("verbose")
                    .takes_value(false)
            ).get_matches();

        let verbose = app.is_present("VERBOSE");

        Ok(Self {
            verbose,
        })
    }
}
