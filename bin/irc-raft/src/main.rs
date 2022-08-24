use std::{net::SocketAddr, sync::atomic::Ordering};

use async_channel::{Receiver, Sender};
use async_executor::Executor;
use async_std::{
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
};
use futures::{io::BufReader, AsyncBufReadExt, AsyncReadExt, FutureExt};
use fxhash::FxHashMap;
use log::{debug, error, info, warn};
use rand::rngs::OsRng;
use smol::future;
use structopt_toml::StructOptToml;

use darkfi::{
    async_daemonize, net,
    raft::{NetMsg, ProtocolRaft, Raft},
    rpc::server::listen_and_serve,
    util::{
        cli::{get_log_config, get_log_level, spawn_config},
        path::{expand_path, get_config_path},
    },
    Error, Result,
};

pub mod crypto;
pub mod privmsg;
pub mod rpc;
pub mod server;
pub mod settings;

use crate::{
    crypto::try_decrypt_message,
    privmsg::Privmsg,
    rpc::JsonRpcInterface,
    server::IrcServerConnection,
    settings::{parse_configured_channels, Args, ChannelInfo, CONFIG_FILE, CONFIG_FILE_CONTENTS},
};

pub type SeenMsgIds = Arc<Mutex<Vec<u32>>>;

fn build_irc_msg(msg: &Privmsg) -> String {
    debug!("ABOUT TO SEND: {:?}", msg);
    let irc_msg =
        format!(":{}!anon@dark.fi PRIVMSG {} :{}\r\n", msg.nickname, msg.channel, msg.message);
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
    info!("Send msg to IRC client '{}' from {}", irc_msg, peer_addr);

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
    autojoin_chans: Vec<String>,
    configured_chans: FxHashMap<String, ChannelInfo>,
) -> Result<()> {
    let (reader, writer) = stream.split();

    let mut reader = BufReader::new(reader);
    let mut conn = IrcServerConnection::new(
        writer,
        seen_msg_id.clone(),
        raft_sender,
        autojoin_chans,
        configured_chans,
    );

    loop {
        let mut line = String::new();
        futures::select! {
            privmsg = raft_receiver.recv().fuse() => {
                let mut msg = privmsg?;
                info!("Received msg from Raft: {:?}", msg);

                let mut smi = seen_msg_id.lock().await;
                if smi.contains(&msg.id) {
                    continue
                }
                smi.push(msg.id);
                drop(smi);

                // Try to potentially decrypt the incoming message.
                if conn.configured_chans.contains_key(&msg.channel) {
                    let chan_info = conn.configured_chans.get(&msg.channel).unwrap();
                    if !chan_info.joined.load(Ordering::Relaxed) {
                        continue
                    }
                    if let Some(salt_box) = &chan_info.salt_box {
                        if let Some(decrypted_msg) = try_decrypt_message(salt_box, &msg.message) {
                            msg.message = decrypted_msg;
                            info!("Decrypted received message: {:?}", msg);
                        }
                    }
                }

                let irc_msg = build_irc_msg(&msg);
                conn.reply(&irc_msg).await?;
            }
            err = reader.read_line(&mut line).fuse() => {
                if let Err(e) = err {
                    warn!("Read line error. Closing stream for {}: {}", peer_addr, e);
                    return Ok(())
                }
                info!("Received msg from IRC client: {:?}", line);
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
    if settings.gen_secret {
        let secret_key = crypto_box::SecretKey::generate(&mut OsRng);
        let encoded = bs58::encode(secret_key.as_bytes());
        println!("{}", encoded.into_string());
        return Ok(())
    }

    let seen_msg_id: SeenMsgIds = Arc::new(Mutex::new(vec![]));

    // Pick up channel settings from the TOML configuration
    let cfg_path = get_config_path(settings.config, CONFIG_FILE)?;
    let configured_chans = parse_configured_channels(&cfg_path)?;

    //
    //Raft
    //
    let datastore_path = expand_path(&settings.datastore)?;
    let net_settings = settings.net;
    let datastore_raft = datastore_path.join("ircd.db");
    let mut raft = Raft::<Privmsg>::new(net_settings.inbound.clone(), datastore_raft)?;
    let raft_sender = raft.get_msgs_channel();
    let raft_receiver = raft.get_commits_channel();

    // P2p setup
    let (p2p_send_channel, p2p_recv_channel) = async_channel::unbounded::<NetMsg>();

    let p2p = net::P2p::new(net_settings.into()).await;
    let p2p = p2p.clone();

    let registry = p2p.protocol_registry();

    let seen_net_msg = Arc::new(Mutex::new(vec![]));
    let raft_node_id = raft.id.clone();
    registry
        .register(net::SESSION_ALL, move |channel, p2p| {
            let raft_node_id = raft_node_id.clone();
            let sender = p2p_send_channel.clone();
            let seen_net_msg_cloned = seen_net_msg.clone();
            async move {
                ProtocolRaft::init(raft_node_id, channel, sender, p2p, seen_net_msg_cloned).await
            }
        })
        .await;

    p2p.clone().start(executor.clone()).await?;

    let executor_cloned = executor.clone();
    let p2p_run_task = executor_cloned.spawn(p2p.clone().run(executor.clone(), None));

    //
    // RPC interface
    //
    let rpc_listen_addr = settings.rpc_listen.clone();
    let rpc_interface =
        Arc::new(JsonRpcInterface { addr: rpc_listen_addr.clone(), p2p: p2p.clone() });
    let rpc_task =
        executor.spawn(async move { listen_and_serve(rpc_listen_addr, rpc_interface).await });

    //
    // IRC instance
    //
    let irc_listen_addr = settings.irc_listen.socket_addrs(|| None)?[0];
    let listener = TcpListener::bind(irc_listen_addr).await?;
    let local_addr = listener.local_addr()?;
    info!("IRC listening on {}", local_addr);
    let executor_cloned = executor.clone();
    let raft_receiver_cloned = raft_receiver.clone();
    let irc_task: smol::Task<Result<()>> = executor.spawn(async move {
        loop {
            let (stream, peer_addr) = match listener.accept().await {
                Ok((s, a)) => (s, a),
                Err(e) => {
                    error!("Failed listening for connections: {}", e);
                    return Err(Error::NetworkServiceStopped)
                }
            };

            info!("IRC Accepted client: {}", peer_addr);

            executor_cloned
                .spawn(process(
                    raft_receiver_cloned.clone(),
                    stream,
                    peer_addr,
                    raft_sender.clone(),
                    seen_msg_id.clone(),
                    settings.autojoin.clone(),
                    configured_chans.clone(),
                ))
                .detach();
        }
    });

    // Run once receive exit signal
    let (signal, shutdown) = async_channel::bounded::<()>(1);
    ctrlc::set_handler(move || {
        warn!(target: "ircd", "ircd start Exit Signal");
        // cleaning up tasks running in the background
        async_std::task::block_on(signal.send(())).unwrap();
        async_std::task::block_on(rpc_task.cancel());
        async_std::task::block_on(irc_task.cancel());
        async_std::task::block_on(p2p_run_task.cancel());
    })
    .unwrap();

    // blocking
    raft.start(p2p.clone(), p2p_recv_channel.clone(), executor.clone(), shutdown.clone()).await?;

    Ok(())
}
