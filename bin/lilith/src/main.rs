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
    urls: Vec<Url>,
    ex: Arc<Executor<'_>>,
) -> Result<()> {
    let mut full_urls = Vec::new();
    for url in &urls {
        let mut url = url.clone();
        url.set_port(Some(info.port))?;
        full_urls.push(url);
    }
    let network_settings = net::Settings {
        inbound: full_urls.clone(),
        seeds: info.seeds,
        peers: info.peers,
        outbound_connections: 0,
        ..Default::default()
    };

    let p2p = net::P2p::new(network_settings).await;

    // Building ext_addr_vec string
    let mut urls_vec = vec![];
    for url in &full_urls {
        urls_vec.push(url.as_ref().to_string());
    }
    info!("Starting seed network node for {} at: {:?}", name, urls_vec);
    p2p.clone().start(ex.clone()).await?;
    let _ex = ex.clone();
    ex.spawn(async move {
        if let Err(e) = p2p.run(_ex, None).await {
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
    ctrlc::set_handler(move || {
        async_std::task::block_on(signal.send(())).unwrap();
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

    // Setting urls
    let mut urls = args.urls.clone();
    if urls.is_empty() {
        info!("Urls are not provided, will use: tcp://127.0.0.1");
        let url = Url::parse("tcp://127.0.0.1")?;
        urls.push(url);
    }

    // Spawn configured networks
    for (name, info) in &configured_nets {
        if let Err(e) = spawn_network(name, info.clone(), urls.clone(), ex.clone()).await {
            error!("Failed starting {} P2P network seed: {}", name, e);
        }
    }

    // Wait for SIGINT
    shutdown.recv().await?;
    print!("\r");
    info!("Caught termination signal, cleaning up and exiting...");

    Ok(())
}
