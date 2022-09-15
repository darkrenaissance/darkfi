use darkfi::cli_desc;

#[derive(clap::Parser)]
#[clap(name = "dnetview", about = cli_desc!(), version)]
pub struct Args {
    #[clap(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    pub verbose: u8,

    /// Logfile path
    #[clap(default_value = "~/.local/darkfi/dnetview.log")]
    pub log_path: String,

    /// Sets a custom config file
    #[clap(short, long)]
    pub config: Option<String>,
}
