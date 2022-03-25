use std::{env, sync::Arc};

extern crate clap;
use async_executor::Executor;
use easy_parallel::Parallel;
use simplelog::{ColorChoice, TermLogger, TerminalMode};

use darkfi::{net, util::cli::log_config, Result};

use crdt::Node;

async fn start(executor: Arc<Executor<'_>>) -> Result<()> {
    //
    // XXX THIS for testing purpose
    //

    let arg = env::args();
    let arg = arg.last().unwrap();

    // node 1
    if arg == "1" {
        let net_settings = net::Settings {
            outbound_connections: 5,
            seeds: vec!["127.0.0.1:9999".parse()?],
            ..Default::default()
        };

        let node1 = Node::new("node1", net_settings).await;

        executor.spawn(node1.clone().start(executor.clone())).detach();

        darkfi::util::sleep(5).await;

        node1.send_event(String::from("hello")).await?;

        loop {}
    }

    // node 2
    if arg == "2" {
        let net_settings = net::Settings {
            inbound: Some("127.0.0.1:6666".parse()?),
            external_addr: Some("127.0.0.1:6666".parse()?),
            seeds: vec!["127.0.0.1:9999".parse()?],
            ..Default::default()
        };
        let node2 = Node::new("node2", net_settings).await;

        node2.start(executor.clone()).await?;
    }

    // seed node
    if arg == "3" {
        let net_settings =
            net::Settings { inbound: Some("127.0.0.1:9999".parse()?), ..Default::default() };

        let node3 = Node::new("node3", net_settings).await;
        node3.start(executor.clone()).await?;
    }
    Ok(())
}

fn main() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = async_channel::unbounded::<()>();

    let (lvl, cfg) = log_config(1)?;

    TermLogger::init(lvl, cfg, TerminalMode::Mixed, ColorChoice::Auto)?;

    let ex2 = ex.clone();
    let (_, result) = Parallel::new()
        .each(0..4, |_| smol::future::block_on(ex.run(shutdown.recv())))
        // Run the main future on the current thread.
        .finish(|| {
            smol::future::block_on(async move {
                start(ex2.clone()).await?;
                drop(signal);
                Ok::<(), darkfi::Error>(())
            })
        });

    result
}
