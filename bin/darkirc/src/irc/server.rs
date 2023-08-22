/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::{fs::File, sync::Arc};

use async_rustls::{rustls, TlsAcceptor};
use log::{error, info};
use smol::{
    io::{self, AsyncRead, AsyncWrite, BufReader},
    lock::Mutex,
    net::{SocketAddr, TcpListener},
};

use darkfi::{
    event_graph::{
        model::{Event, EventId, ModelPtr},
        protocol_event::{Seen, SeenPtr},
        view::ViewPtr,
    },
    net::P2pPtr,
    system::{StoppableTask, SubscriberPtr},
    util::{path::expand_path, time::Timestamp},
    Error, Result,
};

use super::{ClientSubMsg, IrcClient, IrcConfig, NotifierMsg};

use crate::{settings::Args, PrivMsgEvent};

mod nickserv;
use nickserv::NickServ;

const NICK_NICKSERV: &str = "nickserv";

pub struct IrcServer {
    settings: Args,
    p2p: P2pPtr,
    model: ModelPtr<PrivMsgEvent>,
    view: ViewPtr<PrivMsgEvent>,
    clients_subscriptions: SubscriberPtr<ClientSubMsg>,
    seen: SeenPtr<EventId>,
    missed_events: Arc<Mutex<Vec<Event<PrivMsgEvent>>>>,
    /// nickserv service
    pub nickserv: NickServ,
}

impl IrcServer {
    pub async fn new(
        settings: Args,
        p2p: P2pPtr,
        model: ModelPtr<PrivMsgEvent>,
        view: ViewPtr<PrivMsgEvent>,
        clients_subscriptions: SubscriberPtr<ClientSubMsg>,
    ) -> Result<Self> {
        let seen = Seen::new();
        let missed_events = Arc::new(Mutex::new(vec![]));
        Ok(Self {
            settings,
            p2p,
            model,
            view,
            clients_subscriptions,
            seen,
            missed_events,
            nickserv: NickServ::default(),
        })
    }

    pub async fn start(&self, executor: Arc<smol::Executor<'_>>) -> Result<()> {
        let (msg_notifier, msg_recv) = smol::channel::unbounded();

        // Listen to msgs from clients
        StoppableTask::new().start(
            Self::listen_to_msgs(
                self.p2p.clone(),
                self.model.clone(),
                self.seen.clone(),
                msg_recv,
                self.missed_events.clone(),
                self.clients_subscriptions.clone(),
            ),
            |res| async {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => error!(target: "darkirc::irc::server::start", "Failed starting listen to msgs: {}", e),
                }
            },
            Error::DetachedTaskStopped,
            executor.clone(),
        );

        // Listen to msgs from View
        StoppableTask::new().start(
            Self::listen_to_view(
                self.view.clone(),
                self.seen.clone(),
                self.missed_events.clone(),
                self.clients_subscriptions.clone(),
            ),
            |res| async {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => error!(target: "darkirc::irc::server::start", "Failed starting listen to view: {}", e),
                }
            },
            Error::DetachedTaskStopped,
            executor.clone(),
        );

        // Start listening for new connections
        self.listen(msg_notifier, executor).await?;

        Ok(())
    }

    async fn listen_to_view(
        view: ViewPtr<PrivMsgEvent>,
        seen: SeenPtr<EventId>,
        missed_events: Arc<Mutex<Vec<Event<PrivMsgEvent>>>>,
        clients_subscriptions: SubscriberPtr<ClientSubMsg>,
    ) -> Result<()> {
        loop {
            let event = view.lock().await.process().await?;
            if !seen.push(&event.hash()).await {
                continue
            }

            missed_events.lock().await.push(event.clone());

            let msg = event.action.clone();

            clients_subscriptions.notify(ClientSubMsg::Privmsg(msg)).await;
        }
    }

    /// Start listening to msgs from irc clients
    pub async fn listen_to_msgs(
        p2p: P2pPtr,
        model: ModelPtr<PrivMsgEvent>,
        seen: SeenPtr<EventId>,
        recv: smol::channel::Receiver<(NotifierMsg, usize)>,
        missed_events: Arc<Mutex<Vec<Event<PrivMsgEvent>>>>,
        clients_subscriptions: SubscriberPtr<ClientSubMsg>,
    ) -> Result<()> {
        loop {
            let (msg, subscription_id) = recv.recv().await?;

            match msg {
                NotifierMsg::Privmsg(msg) => {
                    // First check if we're communicating with any services.
                    // If not, then we proceed with behaving like it's a normal
                    // message.
                    // TODO: This needs to be protected from adversaries doing
                    //       remote execution.
                    #[allow(clippy::single_match)]
                    match msg.target.to_lowercase().as_str() {
                        NICK_NICKSERV => {
                            //self.nickserv.act(msg);
                            continue
                        }

                        _ => {} // pass
                    }

                    let event = Event {
                        previous_event_hash: model.lock().await.get_head_hash(),
                        action: msg.clone(),
                        timestamp: Timestamp::current_time(),
                    };

                    // Since this will be added to the View directly, other clients connected to irc
                    // server must get informed about this new msg
                    clients_subscriptions
                        .notify_with_exclude(ClientSubMsg::Privmsg(msg), &[subscription_id])
                        .await;

                    if !seen.push(&event.hash()).await {
                        continue
                    }

                    missed_events.lock().await.push(event.clone());

                    p2p.broadcast(&event).await;
                }

                NotifierMsg::UpdateConfig => {
                    //
                    // load and parse the new settings from configuration file and pass it to all
                    // irc clients
                    //
                    // let new_config = IrcConfig::new()?;
                    // clients_subscriptions.notify(ClientSubMsg::Config(new_config)).await;
                }
            }
        }
    }

    /// Start listening to new connections from irc clients
    pub async fn listen(
        &self,
        notifier: smol::channel::Sender<(NotifierMsg, usize)>,
        executor: Arc<smol::Executor<'_>>,
    ) -> Result<()> {
        let (listener, acceptor) = self.setup_listener().await?;
        info!("[IRC SERVER] listening on {}", self.settings.irc_listen);

        loop {
            let (stream, peer_addr) = match listener.accept().await {
                Ok((s, a)) => (s, a),
                Err(e) => {
                    error!("[IRC SERVER] Failed accepting new connections: {}", e);
                    continue
                }
            };

            let result = if let Some(acceptor) = acceptor.clone() {
                // TLS connection
                let stream = match acceptor.accept(stream).await {
                    Ok(s) => s,
                    Err(e) => {
                        error!("[IRC SERVER] Failed accepting TLS connection: {}", e);
                        continue
                    }
                };
                self.process_connection(stream, peer_addr, notifier.clone(), executor.clone()).await
            } else {
                // TCP connection
                self.process_connection(stream, peer_addr, notifier.clone(), executor.clone()).await
            };

            if let Err(e) = result {
                error!("[IRC SERVER] Failed processing connection {}: {}", peer_addr, e);
                continue
            };

            info!("[IRC SERVER] Accept new connection: {}", peer_addr);
        }
    }

    /// On every new connection create new IrcClient
    async fn process_connection<C: AsyncRead + AsyncWrite + Send + Unpin + 'static>(
        &self,
        stream: C,
        peer_addr: SocketAddr,
        notifier: smol::channel::Sender<(NotifierMsg, usize)>,
        executor: Arc<smol::Executor<'_>>,
    ) -> Result<()> {
        let (reader, writer) = io::split(stream);
        let reader = BufReader::new(reader);

        // Subscription for the new client
        let client_subscription = self.clients_subscriptions.clone().subscribe().await;

        // new irc configuration
        let irc_config = IrcConfig::new(&self.settings)?;

        // New irc client
        let mut client = IrcClient::new(
            writer,
            reader,
            peer_addr,
            irc_config,
            notifier,
            client_subscription,
            self.missed_events.clone(),
        );

        // Start listening and detach
        StoppableTask::new().start(
            // Weird hack to prevent lifetimes hell
            async move {client.listen().await; Ok(())},
            |res| async {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => error!(target: "darkirc::irc::server::process_connection", "Failed starting client listen: {}", e),
                }
            },
            Error::DetachedTaskStopped,
            executor,
        );

        Ok(())
    }

    /// Setup a listener for irc server
    async fn setup_listener(&self) -> Result<(TcpListener, Option<TlsAcceptor>)> {
        let listenaddr = self.settings.irc_listen.socket_addrs(|| None)?[0];
        let listener = TcpListener::bind(listenaddr).await?;

        let acceptor = match self.settings.irc_listen.scheme() {
            "tcp+tls" => {
                // openssl genpkey -algorithm ED25519 > example.com.key
                // openssl req -new -out example.com.csr -key example.com.key
                // openssl x509 -req -days 700 -in example.com.csr -signkey example.com.key -out example.com.crt

                if self.settings.irc_tls_secret.is_none() || self.settings.irc_tls_cert.is_none() {
                    error!("[IRC SERVER] To listen using TLS, please set irc_tls_secret and irc_tls_cert in your config file.");
                    return Err(Error::KeypairPathNotFound)
                }

                let file =
                    File::open(expand_path(self.settings.irc_tls_secret.as_ref().unwrap())?)?;
                let mut reader = std::io::BufReader::new(file);
                let secret = &rustls_pemfile::pkcs8_private_keys(&mut reader)?[0];
                let secret = rustls::PrivateKey(secret.clone());

                let file = File::open(expand_path(self.settings.irc_tls_cert.as_ref().unwrap())?)?;
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
}
