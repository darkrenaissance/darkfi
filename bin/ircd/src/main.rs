use async_std::{
    net::TcpListener,
    sync::{Arc, Mutex},
};
use std::{fs::File, net::SocketAddr};

use async_channel::Receiver;
use async_executor::Executor;
use futures::{io::BufReader, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, FutureExt};
use futures_rustls::{rustls, TlsAcceptor};
use fxhash::FxHashMap;
use log::{error, info, warn};
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
        path::get_config_path,
    },
    Error, Result,
};

pub mod crypto;
pub mod privmsg;
pub mod protocol_privmsg;
pub mod rpc;
pub mod server;
pub mod settings;

use crate::{
    privmsg::{Privmsg, PrivmsgsBuffer, SeenMsgIds},
    protocol_privmsg::ProtocolPrivmsg,
    rpc::JsonRpcInterface,
    server::IrcServerConnection,
    settings::{
        parse_configured_channels, parse_configured_contacts, Args, ChannelInfo, CONFIG_FILE,
        CONFIG_FILE_CONTENTS,
    },
};

const SIZE_OF_MSG_IDSS_BUFFER: usize = 65536;
const SIZE_OF_MSGS_BUFFER: usize = 4096;
pub const MAXIMUM_LENGTH_OF_MESSAGE: usize = 1024;
pub const MAXIMUM_LENGTH_OF_NICKNAME: usize = 32;

struct Ircd {
    // msgs
    seen_msg_ids: SeenMsgIds,
    privmsgs_buffer: PrivmsgsBuffer,
    // channels
    autojoin_chans: Vec<String>,
    configured_chans: FxHashMap<String, ChannelInfo>,
    configured_contacts: FxHashMap<String, crypto_box::Box>,
    // p2p
    p2p: net::P2pPtr,
    senders: SubscriberPtr<Privmsg>,
}

impl Ircd {
    fn new(
        seen_msg_ids: SeenMsgIds,
        privmsgs_buffer: PrivmsgsBuffer,
        autojoin_chans: Vec<String>,
        configured_chans: FxHashMap<String, ChannelInfo>,
        configured_contacts: FxHashMap<String, crypto_box::Box>,
        p2p: net::P2pPtr,
    ) -> Self {
        let senders = Subscriber::new();
        Self {
            seen_msg_ids,
            privmsgs_buffer,
            autojoin_chans,
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

        let mut reader = BufReader::new(reader);

        // New subscriber
        let receiver = self.senders.clone().subscribe().await;

        // New irc connection
        let mut conn = IrcServerConnection::new(
            writer,
            peer_addr,
            self.seen_msg_ids.clone(),
            self.privmsgs_buffer.clone(),
            self.autojoin_chans.clone(),
            self.configured_chans.clone(),
            self.configured_contacts.clone(),
            self.p2p.clone(),
            self.senders.clone(),
            receiver.get_id(),
        );

        executor
            .spawn(async move {
                loop {
                    let mut line = String::new();

                    let result: Result<()> = futures::select! {
                        msg = receiver.receive().fuse() => {
                            match conn.process_msg_from_p2p(&msg).await {
                                Ok(_) => Ok(()),
                                Err(e) => {
                                    error!("Process msg from p2p failed {}: {}", peer_addr, e);
                                    Err(Error::ChannelStopped)
                                }
                            }
                        }
                        err = reader.read_line(&mut line).fuse() => {
                            match conn.process_line_from_client(err, line).await {
                                Ok(_) => Ok(()),
                                Err(e) => {
                                    error!("Process line from client failed {}: {}", peer_addr, e);
                                    Err(Error::ChannelStopped)
                                }
                            }
                        }
                    };

                    if let Err(e) = result {
                        warn!("Close connection for clinet {}: {}", peer_addr, e);
                        receiver.unsubscribe().await;
                        break
                    }
                }
            })
            .detach();

        Ok(())
    }
}

async_daemonize!(realmain);
async fn realmain(settings: Args, executor: Arc<Executor<'_>>) -> Result<()> {
    let seen_msg_ids =
        Arc::new(Mutex::new(ringbuffer::AllocRingBuffer::with_capacity(SIZE_OF_MSG_IDSS_BUFFER)));
    let privmsgs_buffer: PrivmsgsBuffer =
        Arc::new(Mutex::new(ringbuffer::AllocRingBuffer::with_capacity(SIZE_OF_MSGS_BUFFER)));

    if settings.gen_secret {
        let secret_key = crypto_box::SecretKey::generate(&mut OsRng);
        let encoded = bs58::encode(secret_key.as_bytes());
        println!("{}", encoded.into_string());
        return Ok(())
    }

    // Pick up channel settings from the TOML configuration
    let cfg_path = get_config_path(settings.config, CONFIG_FILE)?;
    let toml_contents = std::fs::read_to_string(cfg_path)?;
    let configured_chans = parse_configured_channels(&toml_contents)?;
    let configured_contacts = parse_configured_contacts(&toml_contents)?;

    //
    // P2p setup
    //
    let net_settings = settings.net;
    let (p2p_send_channel, p2p_recv_channel) = async_channel::unbounded::<Privmsg>();

    let p2p = net::P2p::new(net_settings.into()).await;
    let p2p = p2p.clone();

    let registry = p2p.protocol_registry();

    let seen_msg_ids_cloned = seen_msg_ids.clone();
    let privmsgs_buffer_cloned = privmsgs_buffer.clone();
    registry
        .register(net::SESSION_ALL, move |channel, p2p| {
            let sender = p2p_send_channel.clone();
            let seen_msg_ids_cloned = seen_msg_ids_cloned.clone();
            let privmsgs_buffer_cloned = privmsgs_buffer_cloned.clone();
            async move {
                ProtocolPrivmsg::init(
                    channel,
                    sender,
                    p2p,
                    seen_msg_ids_cloned,
                    privmsgs_buffer_cloned,
                )
                .await
            }
        })
        .await;

    p2p.clone().start(executor.clone()).await?;

    let executor_cloned = executor.clone();
    executor_cloned.spawn(p2p.clone().run(executor.clone())).detach();

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

    info!("IRC listening on {}", settings.irc_listen);

    let executor_cloned = executor.clone();
    executor
        .spawn(async move {
            let ircd = Ircd::new(
                seen_msg_ids.clone(),
                privmsgs_buffer.clone(),
                settings.autojoin.clone(),
                configured_chans.clone(),
                configured_contacts.clone(),
                p2p.clone(),
            );

            ircd.start_p2p_receive_loop(executor_cloned.clone(), p2p_recv_channel);

            loop {
                let (stream, peer_addr) = match listener.accept().await {
                    Ok((s, a)) => (s, a),
                    Err(e) => {
                        error!("failed accepting new connections: {}", e);
                        continue
                    }
                };

                let result = if let Some(acceptor) = acceptor.clone() {
                    let stream = match acceptor.accept(stream).await {
                        Ok(s) => s,
                        Err(e) => {
                            error!("Failed accepting TLS connection: {}", e);
                            continue
                        }
                    };
                    ircd.process_new_connection(executor_cloned.clone(), stream, peer_addr).await
                } else {
                    ircd.process_new_connection(executor_cloned.clone(), stream, peer_addr).await
                };

                if let Err(e) = result {
                    error!("Failed processing connection {}: {}", peer_addr, e);
                    continue
                };

                info!("IRC Accepted new client: {}", peer_addr);
            }
        })
        .detach();

    // Run once receive exit signal
    let (signal, shutdown) = async_channel::bounded::<()>(1);
    ctrlc_async::set_async_handler(async move {
        warn!(target: "ircd", "ircd start Exit Signal");
        // cleaning up tasks running in the background
        signal.send(()).await.unwrap();
    })
    .unwrap();

    // Wait for SIGINT
    shutdown.recv().await?;
    print!("\r");
    info!("Caught termination signal, cleaning up and exiting...");

    Ok(())
}
