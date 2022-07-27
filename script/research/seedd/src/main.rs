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
use config::{Args, NetOpt};

// TODO: disable unregistered protocols message subscription warning

const CONFIG_FILE: &str = "seedd_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../seedd_config.toml");

async fn spawn_network(
    name: &str,
    mut url: Url,
    opts: NetOpt,
    ex: Arc<Executor<'_>>,
) -> Result<()> {
    url.set_port(Some(opts.port))?;
    let network_settings = net::Settings {
        inbound: Some(url.clone()),
        external_addr: Some(url.clone()),
        seeds: opts.seeds,
        peers: opts.peers,
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

    // Verify any daemon network is enabled
    let check = args.darkfid || args.ircd || args.taud;
    if !check {
        info!("No daemon network is enabled!");
        return Ok(())
    }

    // Spawn darkfid network, if configured
    if args.darkfid {
        if let Err(e) =
            spawn_network("darkfid", args.url.clone(), args.darkfid_opts, ex.clone()).await
        {
            error!("Failed starting darkfid P2P network seed: {}", e);
        }
    }

    // Spawn ircd network, if configured
    if args.ircd {
        if let Err(e) = spawn_network("ircd", args.url.clone(), args.ircd_opts, ex.clone()).await {
            error!("Failed starting ircd P2P network seed: {}", e);
        }
    }

    // Spawn taud network, if configured
    if args.taud {
        if let Err(e) = spawn_network("taud", args.url, args.taud_opts, ex).await {
            error!("Failed starting taud P2P network seed: {}", e);
        }
    }

    // Wait for SIGINT
    shutdown.recv().await?;
    print!("\r");
    info!("Caught termination signal, cleaning up and exiting...");

    Ok(())
}
