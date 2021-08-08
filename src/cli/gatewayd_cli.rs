use crate::Result;

pub struct GatewaydCli {
    pub verbose: bool,
}

impl GatewaydCli {
    pub fn load() -> Result<Self> {
        let app = clap_app!(dfi =>
            (version: "0.1.0")
            (author: "Amir Taaki <amir@dyne.org>")
            (about: "run service daemon")
            (@arg VERBOSE: -v --verbose "Increase verbosity")
        )
        .get_matches();

        let verbose = app.is_present("VERBOSE");

        Ok(Self { verbose })
    }
}
