use async_std::sync::{Arc, Mutex};
use std::fmt;

use async_channel::Receiver;
use async_executor::Executor;

use log::{info, warn};
use rand::rngs::OsRng;
use smol::future;
use structopt_toml::StructOptToml;

use darkfi::{
    async_daemonize, net,
    net::P2pPtr,
    rpc::server::listen_and_serve,
    system::{Subscriber, SubscriberPtr},
    util::{
        cli::{get_log_config, get_log_level, spawn_config},
        expand_path,
        file::save_json_file,
        path::get_config_path,
        sleep,
    },
    Result,
};

pub mod buffers;
pub mod crypto;
pub mod irc;
pub mod privmsg;
pub mod protocol_privmsg;
pub mod rpc;
pub mod settings;

use crate::{
    buffers::{create_buffers, Buffers, RingBuffer, SIZE_OF_MSG_IDSS_BUFFER},
    irc::IrcServer,
    privmsg::Privmsg,
    protocol_privmsg::{LastTerm, ProtocolPrivmsg},
    rpc::JsonRpcInterface,
    settings::{Args, ChannelInfo, CONFIG_FILE, CONFIG_FILE_CONTENTS},
};

const TIMEOUT_FOR_RESEND: u64 = 240;
const SEND_LAST_TERM_MSG: u64 = 4;

#[derive(serde::Serialize)]
struct KeyPair {
    private_key: String,
    public_key: String,
}

impl fmt::Display for KeyPair {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Public key: {}\nPrivate key: {}", self.public_key, self.private_key)
    }
}

async fn resend_unread_msgs(p2p: P2pPtr, buffers: Buffers) -> Result<()> {
    loop {
        sleep(TIMEOUT_FOR_RESEND).await;

        for msg in buffers.unread_msgs.lock().await.msgs.values() {
            p2p.broadcast(msg.clone()).await?;
        }
    }
}

async fn send_last_term(p2p: P2pPtr, buffers: Buffers) -> Result<()> {
    loop {
        sleep(SEND_LAST_TERM_MSG).await;

        let term = buffers.privmsgs.lock().await.last_term();
        p2p.broadcast(LastTerm { term }).await?;
    }
}

struct Ircd {
    notify_clients: SubscriberPtr<Privmsg>,
}

impl Ircd {
    fn new() -> Self {
        let notify_clients = Subscriber::new();
        Self { notify_clients }
    }

    async fn start(
        &self,
        settings: &Args,
        buffers: Buffers,
        p2p: net::P2pPtr,
        p2p_receiver: Receiver<Privmsg>,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        let notify_clients = self.notify_clients.clone();
        executor
            .spawn(async move {
                while let Ok(msg) = p2p_receiver.recv().await {
                    notify_clients.notify(msg).await;
                }
            })
            .detach();

        let irc_server = IrcServer::new(
            settings.clone(),
            buffers.clone(),
            p2p.clone(),
            self.notify_clients.clone(),
        )
        .await?;

        let executor_cloned = executor.clone();
        executor
            .spawn(async move {
                irc_server.start(executor_cloned.clone()).await.unwrap();
            })
            .detach();
        Ok(())
    }
}

async_daemonize!(realmain);
async fn realmain(settings: Args, executor: Arc<Executor<'_>>) -> Result<()> {
    let seen_inv_ids = Arc::new(Mutex::new(RingBuffer::new(SIZE_OF_MSG_IDSS_BUFFER)));
    let buffers = create_buffers();

    if settings.gen_secret {
        let secret_key = crypto_box::SecretKey::generate(&mut OsRng);
        let encoded = bs58::encode(secret_key.as_bytes());
        println!("{}", encoded.into_string());
        return Ok(())
    }

    if settings.gen_keypair {
        let secret_key = crypto_box::SecretKey::generate(&mut OsRng);
        let pub_key = secret_key.public_key();
        let prv_encoded = bs58::encode(secret_key.as_bytes()).into_string();
        let pub_encoded = bs58::encode(pub_key.as_bytes()).into_string();

        let kp = KeyPair { private_key: prv_encoded, public_key: pub_encoded };

        if settings.output.is_some() {
            let datastore = expand_path(&settings.output.unwrap())?;
            save_json_file(&datastore, &kp)?;
        } else {
            println!("Generated KeyPair:\n{}", kp);
        }

        return Ok(())
    }

    //
    // P2p setup
    //
    let mut net_settings = settings.net.clone();
    net_settings.app_version = Some(option_env!("CARGO_PKG_VERSION").unwrap_or("").to_string());
    let (p2p_send_channel, p2p_recv_channel) = async_channel::unbounded::<Privmsg>();

    let p2p = net::P2p::new(net_settings.into()).await;
    let p2p2 = p2p.clone();

    let registry = p2p.protocol_registry();

    let buffers_cloned = buffers.clone();
    let seen_inv_ids_cloned = seen_inv_ids.clone();
    registry
        .register(net::SESSION_ALL, move |channel, p2p| {
            let sender = p2p_send_channel.clone();
            let seen_inv_ids_cloned = seen_inv_ids_cloned.clone();
            let buffers_cloned = buffers_cloned.clone();
            async move {
                ProtocolPrivmsg::init(channel, sender, p2p, seen_inv_ids_cloned, buffers_cloned)
                    .await
            }
        })
        .await;

    p2p.clone().start(executor.clone()).await?;

    let executor_cloned = executor.clone();
    executor_cloned.spawn(p2p.clone().run(executor.clone())).detach();

    //
    // Sync tasks
    //
    executor.spawn(resend_unread_msgs(p2p.clone(), buffers.clone())).detach();
    executor.spawn(send_last_term(p2p.clone(), buffers.clone())).detach();

    //
    // RPC interface
    //
    let rpc_listen_addr = settings.rpc_listen.clone();
    let rpc_interface =
        Arc::new(JsonRpcInterface { addr: rpc_listen_addr.clone(), p2p: p2p.clone() });
    executor.spawn(async move { listen_and_serve(rpc_listen_addr, rpc_interface).await }).detach();

    //
    // IRC instance
    //

    let ircd = Ircd::new();

    ircd.start(&settings, buffers, p2p, p2p_recv_channel, executor.clone()).await?;

    // Run once receive exit signal
    let (signal, shutdown) = async_channel::bounded::<()>(1);
    ctrlc::set_handler(move || {
        warn!(target: "ircd", "ircd start Exit Signal");
        // cleaning up tasks running in the background
        async_std::task::block_on(signal.send(())).unwrap();
    })
    .unwrap();

    // Wait for SIGINT
    shutdown.recv().await?;
    print!("\r");
    info!("Caught termination signal, cleaning up and exiting...");

    p2p2.stop().await;

    Ok(())
}
