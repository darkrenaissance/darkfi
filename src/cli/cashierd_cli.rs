use crate::Result;

pub struct CashierdCli {
    pub verbose: bool,
    pub wallet: bool,
    pub info: bool,
    pub hello: bool,
    pub stop: bool,
}

impl CashierdCli {
    pub fn load() -> Result<Self> {
        let app = clap_app!(dfi =>
            (version: "0.1.0")
            (author: "Dark Renaissance Technologies")
            (about: "run service daemon")
            (@arg VERBOSE: -v --verbose "Increase verbosity")
        )
        .get_matches();

        let verbose = app.is_present("VERBOSE");

        Ok(Self { verbose })
    }
}
