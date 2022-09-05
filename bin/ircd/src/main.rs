use async_std::sync::{Arc, Mutex};
use std::fmt;

use async_channel::Receiver;
use async_executor::Executor;

use fxhash::FxHashMap;
use log::{info, warn};
use rand::rngs::OsRng;
use smol::future;
use structopt_toml::StructOptToml;

use darkfi::{
    async_daemonize, net,
    rpc::server::listen_and_serve,
    system::{Subscriber, SubscriberPtr},
    util::{
        cli::{get_log_config, get_log_level, spawn_config},
        expand_path,
        file::save_json_file,
        path::get_config_path,
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
    buffers::{ArcPrivmsgsBuffer, PrivmsgsBuffer, RingBuffer, SeenIds, SIZE_OF_MSG_IDSS_BUFFER},
    irc::IrcServer,
    privmsg::Privmsg,
    protocol_privmsg::ProtocolPrivmsg,
    rpc::JsonRpcInterface,
    settings::{
        parse_configured_channels, parse_configured_contacts, Args, ChannelInfo, ContactInfo,
        CONFIG_FILE, CONFIG_FILE_CONTENTS,
    },
};

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

pub type UnreadMsgs = Arc<Mutex<FxHashMap<String, Privmsg>>>;

struct Ircd {
    // msgs
    privmsgs_buffer: ArcPrivmsgsBuffer,
    seen_msg_ids: SeenIds,
    // channels
    autojoin_chans: Vec<String>,
    configured_chans: FxHashMap<String, ChannelInfo>,
    configured_contacts: FxHashMap<String, ContactInfo>,
    // p2p
    p2p: net::P2pPtr,
    p2p_notifiers: SubscriberPtr<Privmsg>,
    password: String,
}

impl Ircd {
    fn new(
        privmsgs_buffer: ArcPrivmsgsBuffer,
        seen_msg_ids: SeenIds,
        autojoin_chans: Vec<String>,
        password: String,
        configured_chans: FxHashMap<String, ChannelInfo>,
        configured_contacts: FxHashMap<String, ContactInfo>,
        p2p: net::P2pPtr,
    ) -> Self {
        let p2p_notifiers = Subscriber::new();
        Self {
            privmsgs_buffer,
            seen_msg_ids,
            autojoin_chans,
            password,
            configured_chans,
            configured_contacts,
            p2p,
            p2p_notifiers,
        }
    }

    async fn start(
        &self,
        settings: &Args,
        executor: Arc<Executor<'_>>,
        p2p_receiver: Receiver<Privmsg>,
    ) -> Result<()> {
        let p2p_notifiers = self.p2p_notifiers.clone();
        executor
            .spawn(async move {
                while let Ok(msg) = p2p_receiver.recv().await {
                    p2p_notifiers.notify(msg).await;
                }
            })
            .detach();

        let irc_server = IrcServer::new(
            settings.clone(),
            self.privmsgs_buffer.clone(),
            self.seen_msg_ids.clone(),
            self.autojoin_chans.clone(),
            self.password.clone(),
            self.configured_chans.clone(),
            self.configured_contacts.clone(),
            self.p2p.clone(),
            self.p2p_notifiers.clone(),
        )
        .await?;

        irc_server.start(executor).await?;
        Ok(())
    }
}

async_daemonize!(realmain);
async fn realmain(settings: Args, executor: Arc<Executor<'_>>) -> Result<()> {
    let seen_msg_ids = Arc::new(Mutex::new(RingBuffer::new(SIZE_OF_MSG_IDSS_BUFFER)));
    let seen_inv_ids = Arc::new(Mutex::new(RingBuffer::new(SIZE_OF_MSG_IDSS_BUFFER)));
    let privmsgs_buffer = PrivmsgsBuffer::new();
    let unread_msgs = Arc::new(Mutex::new(FxHashMap::default()));

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

    let password = settings.password.clone().unwrap_or_default();

    // Pick up channel settings from the TOML configuration
    let cfg_path = get_config_path(settings.config.clone(), CONFIG_FILE)?;
    let toml_contents = std::fs::read_to_string(cfg_path)?;
    let configured_chans = parse_configured_channels(&toml_contents)?;
    let configured_contacts = parse_configured_contacts(&toml_contents)?;

    //
    // P2p setup
    //
    let mut net_settings = settings.net.clone();
    net_settings.app_version = option_env!("CARGO_PKG_VERSION").unwrap_or("").to_string();
    let (p2p_send_channel, p2p_recv_channel) = async_channel::unbounded::<Privmsg>();

    let p2p = net::P2p::new(net_settings.into()).await;
    let p2p2 = p2p.clone();

    let registry = p2p.protocol_registry();

    let seen_msg_ids_cloned = seen_msg_ids.clone();
    let seen_inv_ids_cloned = seen_inv_ids.clone();
    let privmsgs_buffer_cloned = privmsgs_buffer.clone();
    let unread_msgs_cloned = unread_msgs.clone();
    registry
        .register(net::SESSION_ALL, move |channel, p2p| {
            let sender = p2p_send_channel.clone();
            let seen_msg_ids_cloned = seen_msg_ids_cloned.clone();
            let seen_inv_ids_cloned = seen_inv_ids_cloned.clone();
            let privmsgs_buffer_cloned = privmsgs_buffer_cloned.clone();
            let unread_msgs_cloned = unread_msgs_cloned.clone();
            async move {
                ProtocolPrivmsg::init(
                    channel,
                    sender,
                    p2p,
                    seen_msg_ids_cloned,
                    seen_inv_ids_cloned,
                    privmsgs_buffer_cloned,
                    unread_msgs_cloned,
                )
                .await
            }
        })
        .await;

    p2p.clone().start(executor.clone()).await?;

    let executor_cloned = executor.clone();
    executor_cloned.spawn(p2p.clone().run(executor.clone())).detach();

    p2p.clone().wait_for_outbound(executor.clone()).await?;

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

    let ircd = Ircd::new(
        privmsgs_buffer.clone(),
        seen_msg_ids.clone(),
        settings.autojoin.clone(),
        password.clone(),
        configured_chans.clone(),
        configured_contacts.clone(),
        p2p.clone(),
    );

    ircd.start(&settings, executor.clone(), p2p_recv_channel).await?;

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
