use async_std::{
    net::TcpListener,
    sync::{Arc, Mutex},
};
use std::{fmt, fs::File, net::SocketAddr};

use async_channel::Receiver;
use async_executor::Executor;
use futures::{
    io::{BufReader, ReadHalf},
    AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, FutureExt,
};
use futures_rustls::{rustls, TlsAcceptor};
use fxhash::FxHashMap;
use log::{error, info, warn};
use rand::rngs::OsRng;
use smol::future;
use structopt_toml::StructOptToml;

use darkfi::{
    async_daemonize, net,
    rpc::server::listen_and_serve,
    system::{Subscriber, SubscriberPtr, Subscription},
    util::{
        cli::{get_log_config, get_log_level, spawn_config},
        expand_path,
        file::save_json_file,
        path::get_config_path,
    },
    Error, Result,
};

pub mod buffers;
pub mod crypto;
pub mod irc_server;
pub mod privmsg;
pub mod protocol_privmsg;
pub mod rpc;
pub mod settings;

use crate::{
    buffers::{ArcPrivmsgsBuffer, PrivmsgsBuffer, RingBuffer, SeenIds, SIZE_OF_MSG_IDSS_BUFFER},
    irc_server::IrcServerConnection,
    privmsg::Privmsg,
    protocol_privmsg::ProtocolPrivmsg,
    rpc::JsonRpcInterface,
    settings::{
        parse_configured_channels, parse_configured_contacts, Args, ChannelInfo, ContactInfo,
        CONFIG_FILE, CONFIG_FILE_CONTENTS,
    },
};

pub const MAXIMUM_LENGTH_OF_MESSAGE: usize = 1024;
pub const MAXIMUM_LENGTH_OF_NICKNAME: usize = 32;

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

async fn setup_listener(settings: Args) -> Result<(TcpListener, Option<TlsAcceptor>)> {

    let listenaddr = settings.irc_listen.socket_addrs(|| None)?[0];
    let listener = TcpListener::bind(listenaddr).await?;

    let acceptor = match settings.irc_listen.scheme() {
        "tls" => {
            // openssl genpkey -algorithm ED25519 > example.com.key
            // openssl req -new -out example.com.csr -key example.com.key
            // openssl x509 -req -days 700 -in example.com.csr -signkey example.com.key -out example.com.crt

            if settings.irc_tls_secret.is_none() || settings.irc_tls_cert.is_none() {
                error!("To listen using TLS, please set irc_tls_secret and irc_tls_cert in your config file.");
                return Err(Error::KeypairPathNotFound)
            }

            let file = File::open(expand_path(&settings.irc_tls_secret.unwrap())?)?;
            let mut reader = std::io::BufReader::new(file);
            let secret = &rustls_pemfile::pkcs8_private_keys(&mut reader)?[0];
            let secret = rustls::PrivateKey(secret.clone());

            let file = File::open(expand_path(&settings.irc_tls_cert.unwrap())?)?;
            let mut reader = std::io::BufReader::new(file);
            let certificate = &rustls_pemfile::certs(&mut reader)?[0];
            let certificate = rustls::Certificate(certificate.clone());

            let config = rustls::ServerConfig::builder()
                .with_safe_defaults()
                .with_no_client_auth()
                .with_single_cert(vec![certificate], secret)?;

            let acceptor = TlsAcceptor::from(Arc::new(config));
            Some(acceptor)
        }
        _ => None,
    };
    Ok((listener, acceptor))
}

async fn start_listening(ircd: Ircd, executor: Arc<Executor<'_>>, settings: Args) -> Result<()> {
    let (listener, acceptor) = setup_listener(settings.clone()).await?;
    info!("[IRC SERVER] listening on {}", settings.irc_listen);
    loop {
        let (stream, peer_addr) = match listener.accept().await {
            Ok((s, a)) => (s, a),
            Err(e) => {
                error!("[IRC SERVER] Failed accepting new connections: {}", e);
                continue
            }
        };

        let result = if let Some(acceptor) = acceptor.clone() {
            let stream = match acceptor.accept(stream).await {
                Ok(s) => s,
                Err(e) => {
                    error!("[IRC SERVER] Failed accepting TLS connection: {}", e);
                    continue
                }
            };
            ircd.process_new_connection(executor.clone(), stream, peer_addr).await
        } else {
            ircd.process_new_connection(executor.clone(), stream, peer_addr).await
        };

        if let Err(e) = result {
            error!("[IRC SERVER] Failed processing connection {}: {}", peer_addr, e);
            continue
        };

        info!("[IRC SERVER] Accept new connection: {}", peer_addr);
    }
}

struct Ircd {
    // msgs
    seen_msg_ids: SeenIds,
    privmsgs_buffer: ArcPrivmsgsBuffer,
    // channels
    autojoin_chans: Vec<String>,
    configured_chans: FxHashMap<String, ChannelInfo>,
    configured_contacts: FxHashMap<String, ContactInfo>,
    // p2p
    p2p: net::P2pPtr,
    senders: SubscriberPtr<Privmsg>,
    password: String,
}

impl Ircd {
    fn new(
        seen_msg_ids: SeenIds,
        privmsgs_buffer: ArcPrivmsgsBuffer,
        autojoin_chans: Vec<String>,
        password: String,
        configured_chans: FxHashMap<String, ChannelInfo>,
        configured_contacts: FxHashMap<String, ContactInfo>,
        p2p: net::P2pPtr,
    ) -> Self {
        let senders = Subscriber::new();
        Self {
            seen_msg_ids,
            privmsgs_buffer,
            autojoin_chans,
            password,
            configured_chans,
            configured_contacts,
            p2p,
            senders,
        }
    }

    fn start_p2p_receive_loop(&self, executor: Arc<Executor<'_>>, p2p_receiver: Receiver<Privmsg>) {
        let senders = self.senders.clone();
        executor
            .spawn(async move {
                while let Ok(msg) = p2p_receiver.recv().await {
                    senders.notify(msg).await;
                }
            })
            .detach();
    }

    async fn process_new_connection<C: AsyncRead + AsyncWrite + Send + Unpin + 'static>(
        &self,
        executor: Arc<Executor<'_>>,
        stream: C,
        peer_addr: SocketAddr,
    ) -> Result<()> {
        let (reader, writer) = stream.split();

        let reader = BufReader::new(reader);

        // New subscriber
        let receiver = self.senders.clone().subscribe().await;

        // New irc connection
        let conn = IrcServerConnection::new(
            writer,
            peer_addr,
            self.seen_msg_ids.clone(),
            self.privmsgs_buffer.clone(),
            self.autojoin_chans.clone(),
            self.password.clone(),
            self.configured_chans.clone(),
            self.configured_contacts.clone(),
            self.p2p.clone(),
            self.senders.clone(),
            receiver.get_id(),
        );

        executor.spawn(Self::listen(conn, reader, receiver)).detach();

        Ok(())
    }

    async fn listen<C: AsyncRead + AsyncWrite + Send + Unpin + 'static>(
        mut conn: IrcServerConnection<C>,
        mut reader: BufReader<ReadHalf<C>>,
        receiver: Subscription<Privmsg>,
    ) -> Result<()> {
        loop {
            let mut line = String::new();

            futures::select! {
                msg = receiver.receive().fuse() => {
                    if let Err(e) = conn.process_msg_from_p2p(&msg).await {
                        error!("Process msg from p2p failed {}: {}",  conn.peer_address, e);
                        break
                    }
                }
                err = reader.read_line(&mut line).fuse() => {
                    if let Err(e) = conn.process_line_from_client(err, line).await {
                        error!("Process line from client failed {}: {}", conn.peer_address, e);
                        break
                    }
                }
            }
        }

        warn!("Close connection for: {}", conn.peer_address);
        receiver.unsubscribe().await;
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
        seen_msg_ids.clone(),
        privmsgs_buffer.clone(),
        settings.autojoin.clone(),
        password.clone(),
        configured_chans.clone(),
        configured_contacts.clone(),
        p2p.clone(),
    );

    ircd.start_p2p_receive_loop(executor.clone(), p2p_recv_channel);
    executor.spawn(start_listening(ircd, executor.clone(), settings.clone())).detach();

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
