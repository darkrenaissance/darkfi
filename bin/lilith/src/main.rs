use async_executor::Executor;
use async_std::sync::Arc;
use futures_lite::future;
use log::{error, info};
use structopt_toml::StructOptToml;
use url::Url;

use darkfi::{
    async_daemonize, net,
    util::{
        cli::{get_log_config, get_log_level, spawn_config},
        path::get_config_path,
    },
    Result,
};

mod config;
use config::{parse_configured_networks, Args, NetInfo};

const CONFIG_FILE: &str = "lilith_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../lilith_config.toml");

async fn spawn_network(
    name: &str,
    info: NetInfo,
    mut url: Url,
    ex: Arc<Executor<'_>>,
) -> Result<()> {
    url.set_port(Some(info.port))?;
    let network_settings = net::Settings {
        inbound: Some(url.clone()),
        seeds: info.seeds,
        peers: info.peers,
        outbound_connections: 0,
        ..Default::default()
    };

    let p2p = net::P2p::new(network_settings).await;

    info!("Starting seed network node for {} at: {}", name, url);
    p2p.clone().start(ex.clone()).await?;
    let _ex = ex.clone();
    ex.spawn(async move {
        if let Err(e) = p2p.run(_ex).await {
            error!("Failed starting P2P network seed: {}", e);
        }
    })
    .detach();

    Ok(())
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<Executor<'_>>) -> Result<()> {
    // We use this handler to block this function after detaching all
    // tasks, and to catch a shutdown signal, where we can clean up and
    // exit gracefully.
    let (signal, shutdown) = async_channel::bounded::<()>(1);
    ctrlc_async::set_async_handler(async move {
        signal.send(()).await.unwrap();
    })
    .unwrap();

    // Pick up network settings from the TOML configuration
    let cfg_path = get_config_path(args.config, CONFIG_FILE)?;
    let toml_contents = std::fs::read_to_string(cfg_path)?;
    let configured_nets = parse_configured_networks(&toml_contents)?;

    // Verify any daemon network is enabled
    if configured_nets.is_empty() {
        info!("No daemon network is enabled!");
        return Ok(())
    }

    // Spawn configured networks
    for (name, info) in &configured_nets {
        if let Err(e) = spawn_network(name, info.clone(), args.url.clone(), ex.clone()).await {
            error!("Failed starting {} P2P network seed: {}", name, e);
        }
    }

    // Wait for SIGINT
    shutdown.recv().await?;
    print!("\r");
    info!("Caught termination signal, cleaning up and exiting...");

    Ok(())
}
