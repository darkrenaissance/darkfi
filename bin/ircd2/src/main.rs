use async_std::net::{TcpListener, TcpStream};
use std::{net::SocketAddr, sync::Arc};

use async_channel::Receiver;
use async_executor::Executor;
use clap::Parser;
use easy_parallel::Parallel;
use futures::{io::BufReader, AsyncBufReadExt, AsyncReadExt, FutureExt};
use log::{debug, error, info, warn};
use simplelog::{ColorChoice, TermLogger, TerminalMode};

use darkfi::{
    cli_desc, net,
    raft::Raft,
    rpc::rpcserver::{listen_and_serve, RpcServerConfig},
    util::cli::log_config,
    Error, Result,
};

pub(crate) mod privmsg;
pub(crate) mod rpc;
pub(crate) mod server;

use crate::{privmsg::Privmsg, rpc::JsonRpcInterface, server::IrcServerConnection};

#[derive(Parser)]
#[clap(name = "ircd", about = cli_desc!(), version)]
struct Args {
    /// Accept address
    #[clap(short, long)]
    accept: Option<SocketAddr>,

    /// Seed node (repeatable)
    #[clap(short, long)]
    seed: Vec<SocketAddr>,

    /// Manual connection (repeatable)
    #[clap(short, long)]
    connect: Vec<SocketAddr>,

    /// Connection slots
    #[clap(long, default_value_t = 0)]
    slots: u32,

    /// External address
    #[clap(short, long)]
    external: Option<SocketAddr>,

    /// IRC listen address
    #[clap(short = 'r', long, default_value = "127.0.0.1:6667")]
    irc: SocketAddr,

    /// RPC listen address
    #[clap(long, default_value = "127.0.0.1:8000")]
    rpc: SocketAddr,

    /// Verbosity level
    #[clap(short, parse(from_occurrences))]
    verbose: u8,
}

async fn process_user_input(
    mut line: String,
    peer_addr: SocketAddr,
    conn: &mut IrcServerConnection,
    sender: async_channel::Sender<Privmsg>,
) -> Result<()> {
    if line.is_empty() {
        warn!("Received empty line from {}. Closing connection.", peer_addr);
        return Err(Error::ChannelStopped)
    }

    assert!(&line[(line.len() - 2)..] == "\r\n");
    // Remove CRLF
    line.pop();
    line.pop();

    debug!("Received '{}' from {}", line, peer_addr);

    if let Err(e) = conn.update(line, sender).await {
        warn!("Connection error: {} for {}", e, peer_addr);
        return Err(Error::ChannelStopped)
    }

    Ok(())
}

async fn process(
    receiver: Receiver<Privmsg>,
    stream: TcpStream,
    peer_addr: SocketAddr,
    sender: async_channel::Sender<Privmsg>,
) -> Result<()> {
    let (reader, writer) = stream.split();

    let mut reader = BufReader::new(reader);
    let mut conn = IrcServerConnection::new(writer);

    loop {
        let mut line = String::new();
        futures::select! {
            privmsg = receiver.recv().fuse() => {
                let msg = privmsg.expect("internal message queue error");
                debug!("ABOUT TO SEND: {:?}", msg);
                let irc_msg = format!(":{}!anon@dark.fi PRIVMSG {} :{}\r\n",
                                      msg.nickname,
                                      msg.channel,
                                      msg.message,
                                      );

                conn.reply(&irc_msg).await?;
            }

            err = reader.read_line(&mut line).fuse() => {
                if let Err(e) = err {
                    warn!("Read line error. Closing stream for {}: {}", peer_addr, e);
                    return Ok(())
                }

                process_user_input(line, peer_addr, &mut conn, sender.clone()).await?;
            }
        };
    }
}

async fn start(executor: Arc<Executor<'_>>, args: Args, net_settings: net::Settings) -> Result<()> {
    let listener = TcpListener::bind(args.irc).await?;
    let local_addr = listener.local_addr()?;
    info!("Listening on {}", local_addr);

    //
    // Raft
    //
    let mut raft = Raft::<Privmsg>::new(net_settings.inbound, std::path::PathBuf::from("msgs.db"))?;

    let raft_sender = raft.get_broadcast();
    let commits = raft.get_commits();

    //
    // RPC interface

    let rpc_config = RpcServerConfig {
        socket_addr: args.rpc,
        // TODO: Use net/transport:
        use_tls: false,
        identity_path: Default::default(),
        identity_pass: Default::default(),
    };
    let executor_cloned = executor.clone();
    let rpc_interface = Arc::new(JsonRpcInterface { addr: args.rpc });
    let rpc_task = executor.spawn(async move {
        listen_and_serve(rpc_config, rpc_interface, executor_cloned.clone()).await
    });

    //
    // IRC instance
    //
    let executor_cloned = executor.clone();
    let irc_task: smol::Task<Result<()>> = executor.spawn(async move {
        loop {
            let (stream, peer_addr) = match listener.accept().await {
                Ok((s, a)) => (s, a),
                Err(e) => {
                    error!("Failed listening for connections: {}", e);
                    return Err(Error::ServiceStopped)
                }
            };

            info!("Accepted client: {}", peer_addr);

            executor_cloned
                .spawn(process(commits.clone(), stream, peer_addr, raft_sender.clone()))
                .detach();
        }
    });

    let stop_signal = async_channel::bounded::<()>(10);

    ctrlc_async::set_async_handler(async move {
        warn!(target: "ircd", "ircd start() Exit Signal");
        // cleaning up tasks running in the background
        stop_signal.0.send(()).await.expect("send exit signal to raft");
        rpc_task.cancel().await;
        irc_task.cancel().await;
    })
    .expect("handle exit signal");

    // blocking
    raft.start(net_settings.clone(), executor.clone(), stop_signal.1.clone()).await?;

    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();

    let (lvl, conf) = log_config(args.verbose.into())?;
    TermLogger::init(lvl, conf, TerminalMode::Mixed, ColorChoice::Auto)?;

    let net_settings = net::Settings {
        inbound: args.accept,
        outbound_connections: args.slots,
        external_addr: args.external,
        peers: args.connect.clone(),
        seeds: args.seed.clone(),
        ..Default::default()
    };

    let ex = Arc::new(Executor::new());
    let ex_clone = ex.clone();
    let (signal, shutdown) = async_channel::unbounded::<()>();
    let (_, result) = Parallel::new()
        .each(0..4, |_| smol::future::block_on(ex.run(shutdown.recv())))
        // Run the main future on the current thread.
        .finish(|| {
            smol::future::block_on(async move {
                start(ex_clone.clone(), args, net_settings).await?;
                drop(signal);
                Ok::<(), darkfi::Error>(())
            })
        });

    result
}
