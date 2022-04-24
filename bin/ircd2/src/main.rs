use async_std::net::{TcpListener, TcpStream};
use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use async_channel::Receiver;
use async_executor::Executor;
use easy_parallel::Parallel;
use futures::{io::BufReader, AsyncBufReadExt, AsyncReadExt, FutureExt};
use log::{debug, error, info, warn};
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use smol::future;
use structopt_toml::StructOptToml;

use darkfi::{
    async_daemonize, net,
    raft::Raft,
    rpc::rpcserver::{listen_and_serve, RpcServerConfig},
    util::{
        cli::{log_config, spawn_config},
        path::get_config_path,
    },
    Error, Result,
};

pub mod privmsg;
pub mod rpc;
pub mod server;
pub mod settings;

use crate::{
    privmsg::Privmsg,
    rpc::JsonRpcInterface,
    server::IrcServerConnection,
    settings::{Args, CONFIG_FILE, CONFIG_FILE_CONTENTS},
};

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
                let msg = privmsg?;
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

async_daemonize!(realmain);
async fn realmain(settings: Args, executor: Arc<Executor<'_>>) -> Result<()> {
    let listener = TcpListener::bind(settings.irc_listen).await?;
    let local_addr = listener.local_addr()?;
    info!("Listening on {}", local_addr);

    let p2p_settings = net::Settings {
        inbound: settings.accept,
        outbound_connections: settings.slots,
        external_addr: settings.accept,
        peers: settings.connect.clone(),
        seeds: settings.seeds.clone(),
        ..Default::default()
    };

    let datastore_path = PathBuf::from(&settings.datastore);

    //
    //Raft
    //
    let datastore_raft = datastore_path.join("ircd.db");

    let mut raft = Raft::<Privmsg>::new(settings.accept, datastore_raft)?;

    let raft_sender = raft.get_broadcast();
    let commits = raft.get_commits();

    //
    // RPC interface
    //
    let rpc_config = RpcServerConfig {
        socket_addr: settings.rpc_listen,
        // TODO: Use net/transport:
        use_tls: false,
        identity_path: Default::default(),
        identity_pass: Default::default(),
    };
    let executor_cloned = executor.clone();
    let rpc_interface = Arc::new(JsonRpcInterface { addr: settings.rpc_listen });
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

    let (signal, shutdown) = async_channel::bounded::<()>(1);
    ctrlc_async::set_async_handler(async move {
        warn!(target: "ircd", "ircd start Exit Signal");
        // cleaning up tasks running in the background
        signal.send(()).await.unwrap();
        rpc_task.cancel().await;
        irc_task.cancel().await;
    })
    .unwrap();

    // blocking
    raft.start(p2p_settings.clone(), executor.clone(), shutdown.clone()).await?;

    Ok(())
}
