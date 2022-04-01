use std::net::SocketAddr;

use async_executor::Executor;
use async_std::sync::Arc;
use clap::Parser;
use easy_parallel::Parallel;
use simplelog::{ColorChoice, Config, LevelFilter, TermLogger, TerminalMode};

use sraft::{Raft, RaftRpc};

#[derive(Parser)]
struct Args {
    #[clap(long, short)]
    peer: Vec<SocketAddr>,

    #[clap(long, short)]
    id: u64,

    #[clap(long, short)]
    listen: SocketAddr,
}

#[async_std::main]
async fn main() {
    let args = Args::parse();

    TermLogger::init(LevelFilter::Debug, Config::default(), TerminalMode::Mixed, ColorChoice::Auto)
        .unwrap();

    let mut raft = Raft::new(args.id);
    for (k, v) in args.peer.iter().enumerate() {
        raft.peers.insert(k as u64, *v);
    }

    let raft_rpc = RaftRpc(args.listen);

    let ex = Arc::new(Executor::new());
    let (_signal, shutdown) = async_channel::unbounded::<()>();

    Parallel::new()
        .each(0..4, |_| smol::future::block_on(ex.run(shutdown.recv())))
        //
        .add(|| {
            smol::future::block_on(async move {
                raft_rpc.start().await;
            });
            Ok(())
        })
        //
        .finish(|| {
            smol::future::block_on(async move {
                raft.start().await;
            })
        });
}
