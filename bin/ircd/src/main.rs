use async_std::{
    net::TcpStream,
    sync::{Arc, Mutex},
};
use std::net::SocketAddr;

use async_channel::{Receiver, Sender};
use async_executor::Executor;
use easy_parallel::Parallel;
use futures::{io::BufReader, AsyncBufReadExt, AsyncReadExt, FutureExt, StreamExt};
use log::{debug, info, warn};
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use smol::future;
use structopt_toml::StructOptToml;

use darkfi::{
    async_daemonize,
    net::transport::{TcpTransport, Transport},
    raft::Raft,
    rpc::rpcserver::{listen_and_serve, RpcServerConfig},
    util::{
        cli::{log_config, spawn_config},
        path::{expand_path, get_config_path},
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

pub type SeenMsgIds = Arc<Mutex<Vec<u32>>>;

fn build_irc_msg(msg: &Privmsg) -> String {
    debug!("ABOUT TO SEND: {:?}", msg);
    let irc_msg =
        format!(":{}!anon@dark.fi PRIVMSG {} :{}\r\n", msg.nickname, msg.channel, msg.message,);
    irc_msg
}

fn clean_input(mut line: String, peer_addr: &SocketAddr) -> Result<String> {
    if line.is_empty() {
        warn!("Received empty line from {}. ", peer_addr);
        warn!("Closing connection.");
        return Err(Error::ChannelStopped)
    }

    if &line[(line.len() - 2)..] != "\r\n" {
        warn!("Closing connection.");
        return Err(Error::ChannelStopped)
    }
    // Remove CRLF
    line.pop();
    line.pop();

    Ok(line)
}

async fn broadcast_msg(
    irc_msg: String,
    peer_addr: SocketAddr,
    conn: &mut IrcServerConnection,
) -> Result<()> {
    info!("Send msg to IRC server '{}' from {}", irc_msg, peer_addr);

    if let Err(e) = conn.update(irc_msg).await {
        warn!("Connection error: {} for {}", e, peer_addr);
        return Err(Error::ChannelStopped)
    }

    Ok(())
}

async fn process(
    raft_receiver: Receiver<Privmsg>,
    stream: TcpStream,
    peer_addr: SocketAddr,
    raft_sender: Sender<Privmsg>,
    seen_msg_id: SeenMsgIds,
) -> Result<()> {
    let (reader, writer) = stream.split();

    let mut reader = BufReader::new(reader);
    let mut conn = IrcServerConnection::new(writer, seen_msg_id.clone(), raft_sender);

    loop {
        let mut line = String::new();
        futures::select! {
            privmsg = raft_receiver.recv().fuse() => {
                info!("Receive msg from raft");
                let msg = privmsg?;

                let mut smi = seen_msg_id.lock().await;
                if smi.contains(&msg.id) {
                    continue
                }
                smi.push(msg.id);
                drop(smi);

                let irc_msg = build_irc_msg(&msg);
                conn.reply(&irc_msg).await?;
            }
            err = reader.read_line(&mut line).fuse() => {
                if let Err(e) = err {
                    warn!("Read line error. Closing stream for {}: {}", peer_addr, e);
                    return Ok(())
                }
                info!("Receive msg from IRC server");
                let irc_msg = match clean_input(line, &peer_addr) {
                    Ok(m) => m,
                    Err(e) => return Err(e)
                };
                broadcast_msg(irc_msg, peer_addr,&mut conn).await?;
            }
        };
    }
}

async_daemonize!(realmain);
async fn realmain(settings: Args, executor: Arc<Executor<'_>>) -> Result<()> {
    let seen_msg_id: SeenMsgIds = Arc::new(Mutex::new(vec![]));

    //
    //Raft
    //
    let datastore_path = expand_path(&settings.datastore)?;
    let net_settings = settings.net;
    let datastore_raft = datastore_path.join("ircd.db");
    let mut raft = Raft::<Privmsg>::new(net_settings.inbound, datastore_raft)?;
    let raft_sender = raft.get_broadcast();
    let raft_receiver = raft.get_commits();

    //
    // RPC interface
    //
    let rpc_config = RpcServerConfig {
        socket_addr: settings.rpc_listen,
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
        let irc_listen_url = url::Url::parse(&settings.irc_listen)?;

        let transport = TcpTransport::new(None, 1024);
        let listener = transport.listen_on(irc_listen_url.clone()).unwrap().await.unwrap();
        let mut incoming = listener.incoming();
        info!("IRC start a TCP connection {}", &irc_listen_url.to_string());
        while let Some(stream) = incoming.next().await {
            let stream = stream.unwrap();
            let peer_addr = stream.peer_addr()?;
            info!("IRC Accepted TCP connection {}", peer_addr);
            executor_cloned
                .spawn(process(
                    raft_receiver.clone(),
                    stream,
                    peer_addr,
                    raft_sender.clone(),
                    seen_msg_id.clone(),
                ))
                .detach();
        }

        Ok(())
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
    raft.start(net_settings.into(), executor.clone(), shutdown.clone()).await?;

    Ok(())
}
