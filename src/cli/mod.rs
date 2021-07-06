pub mod client_cli;
pub mod service_cli;

pub use client_cli::{cli_config, darkfid_cli::DarkfidCli};
pub use service_cli::ServiceCli;
