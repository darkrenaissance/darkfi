use std::{net::SocketAddr, sync::Arc};

use async_channel::Receiver;
use async_executor::Executor;
use async_std::net::{TcpListener, TcpStream};
use clap::Parser;
use easy_parallel::Parallel;
use futures::{io::BufReader, AsyncBufReadExt, AsyncReadExt, FutureExt};
use log::{debug, error, info, warn};
use simplelog::{ColorChoice, TermLogger, TerminalMode};

use darkfi::{
    cli_desc, net,
    rpc::rpcserver::{listen_and_serve, RpcServerConfig},
    util::cli::log_config,
    Error, Result,
};

pub(crate) mod proto;
pub(crate) mod rpc;
pub(crate) mod server;

use crate::{
    proto::privmsg::{Privmsg, ProtocolPrivmsg, SeenPrivmsgIds, SeenPrivmsgIdsPtr},
    rpc::JsonRpcInterface,
    server::IrcServerConnection,
};

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
    p2p: net::P2pPtr,
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

    if let Err(e) = conn.update(line, p2p.clone()).await {
        warn!("Connection error: {} for {}", e, peer_addr);
        return Err(Error::ChannelStopped)
    }

    Ok(())
}

async fn process(
    receiver: Receiver<Arc<Privmsg>>,
    stream: TcpStream,
    peer_addr: SocketAddr,
    p2p: net::P2pPtr,
    seen_privmsg_ids: SeenPrivmsgIdsPtr,
) -> Result<()> {
    let (reader, writer) = stream.split();

    let mut reader = BufReader::new(reader);
    let mut conn = IrcServerConnection::new(writer, seen_privmsg_ids);

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

                process_user_input(line, peer_addr, &mut conn, p2p.clone()).await?;
            }
        };
    }
}

async fn start(executor: Arc<Executor<'_>>, args: Args, net_settings: net::Settings) -> Result<()> {
    let listener = TcpListener::bind(args.irc).await?;
    let local_addr = listener.local_addr()?;
    info!("Listening on {}", local_addr);

    let rpc_config = RpcServerConfig {
        socket_addr: args.rpc,
        // TODO: Use net/transport:
        use_tls: false,
        identity_path: Default::default(),
        identity_pass: Default::default(),
    };

    //
    // Privmsg protocol
    //
    let seen_privmsg_ids = SeenPrivmsgIds::new();
    let seen_privmsg_ids_clone = seen_privmsg_ids.clone();

    let (sender, receiver) = async_channel::unbounded();
    let sender_clone = sender.clone();

    let p2p = net::P2p::new(net_settings).await;
    let registry = p2p.protocol_registry();
    registry
        .register(!net::SESSION_SEED, move |channel, p2p| {
            let sender = sender_clone.clone();
            let seen_privmsg_ids = seen_privmsg_ids_clone.clone();
            async move { ProtocolPrivmsg::init(channel, sender, seen_privmsg_ids, p2p).await }
        })
        .await;

    //
    // P2P network main instance
    //
    p2p.clone().start(executor.clone()).await?;
    let executor_clone = executor.clone();
    let p2p_clone = p2p.clone();
    executor
        .spawn(async move {
            if let Err(e) = p2p_clone.run(executor_clone).await {
                error!("P2P run failed: {}", e);
            }
        })
        .detach();

    //
    // RPC interface
    let executor_clone = executor.clone();
    let rpc_interface = Arc::new(JsonRpcInterface { p2p: p2p.clone(), addr: args.rpc });
    executor
        .spawn(async move { listen_and_serve(rpc_config, rpc_interface, executor_clone.clone()).await })
        .detach();

    //
    // IRC instance
    //
    loop {
        let (stream, peer_addr) = match listener.accept().await {
            Ok((s, a)) => (s, a),
            Err(e) => {
                error!("Failed listening for connections: {}", e);
                return Err(Error::ServiceStopped)
            }
        };

        info!("Accepted client: {}", peer_addr);

        let p2p_clone = p2p.clone();
        executor
            .spawn(process(
                receiver.clone(),
                stream,
                peer_addr,
                p2p_clone,
                seen_privmsg_ids.clone(),
            ))
            .detach();
    }
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
