use std::sync::Arc;

use simplelog::{ColorChoice, LevelFilter, TermLogger, TerminalMode};
use url::Url;

use darkfi::{rpc::server::listen_and_serve, Result};

mod dao_contract;
mod example_contract;
mod money_contract;
mod rpc;

mod demo;
mod note;

use crate::rpc::JsonRpcInterface;
// use crate::demo::demo;

async fn start() -> Result<()> {
    let rpc_addr = Url::parse("tcp://127.0.0.1:7777")?;
    let rpc_interface = Arc::new(JsonRpcInterface {});

    listen_and_serve(rpc_addr, rpc_interface).await?;
    Ok(())
}

#[async_std::main]
async fn main() -> Result<()> {
    TermLogger::init(
        LevelFilter::Debug,
        simplelog::Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )?;

    start().await?;
    // demo().await.unwrap();
    Ok(())
}
