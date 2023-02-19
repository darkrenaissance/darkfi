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

use std::{collections::HashMap, fs::File, net::SocketAddr};

use async_std::{
    net::TcpListener,
    sync::{Arc, Mutex},
};
use futures::{io::BufReader, AsyncRead, AsyncReadExt, AsyncWrite};
use futures_rustls::{rustls, TlsAcceptor};
use log::{error, info};

use darkfi::{
    event_graph::{
        get_current_time,
        model::{Event, EventId, ModelPtr},
        protocol_event::{Seen, SeenPtr, UnreadEventsPtr},
        view::ViewPtr,
        EventAction, PrivMsgEvent,
    },
    net::P2pPtr,
    system::SubscriberPtr,
    util::path::expand_path,
    Error, Result,
};

use crate::settings::{Args, ChannelInfo, ContactInfo};

mod client;

pub use client::IrcClient;

#[derive(Clone)]
pub struct IrcConfig {
    // init bool
    pub is_nick_init: bool,
    pub is_user_init: bool,
    pub is_registered: bool,
    pub is_cap_end: bool,
    pub is_pass_init: bool,

    // user config
    pub nickname: String,
    pub password: String,
    pub private_key: Option<String>,
    pub capabilities: HashMap<String, bool>,

    // channels and contacts
    pub channels: HashMap<String, ChannelInfo>,
    pub contacts: HashMap<String, ContactInfo>,
}

impl IrcConfig {
    pub fn new(settings: &Args) -> Result<Self> {
        let password = settings.password.as_ref().unwrap_or(&String::new()).clone();
        let private_key = settings.private_key.clone();

        let mut channels = settings.channels.clone();

        for chan in settings.autojoin.iter() {
            if !channels.contains_key(chan) {
                channels.insert(chan.clone(), ChannelInfo::new());
            }
        }

        let contacts = settings.contacts.clone();

        let mut capabilities = HashMap::new();
        capabilities.insert("no-history".to_string(), false);

        Ok(Self {
            is_nick_init: false,
            is_user_init: false,
            is_registered: false,
            is_cap_end: true,
            is_pass_init: false,
            nickname: "anon".to_string(),
            password,
            channels,
            contacts,
            private_key,
            capabilities,
        })
    }
}

#[derive(Clone)]
pub enum ClientSubMsg {
    Privmsg(PrivMsgEvent),
    Config(IrcConfig),
}
#[derive(Clone)]
pub enum NotifierMsg {
    Privmsg(PrivMsgEvent),
    UpdateConfig,
}

pub struct IrcServer {
    settings: Args,
    p2p: P2pPtr,
    model: ModelPtr,
    view: ViewPtr,
    unread_events: UnreadEventsPtr,
    clients_subscriptions: SubscriberPtr<ClientSubMsg>,
    seen: SeenPtr<EventId>,
    missed_events: Arc<Mutex<Vec<Event>>>,
}

impl IrcServer {
    pub async fn new(
        settings: Args,
        p2p: P2pPtr,
        model: ModelPtr,
        view: ViewPtr,
        unread_events: UnreadEventsPtr,
        clients_subscriptions: SubscriberPtr<ClientSubMsg>,
    ) -> Result<Self> {
        let seen = Seen::new();
        let missed_events = Arc::new(Mutex::new(vec![]));
        Ok(Self {
            settings,
            p2p,
            model,
            view,
            unread_events,
            clients_subscriptions,
            seen,
            missed_events,
        })
    }
    pub async fn start(&self, executor: Arc<smol::Executor<'_>>) -> Result<()> {
        let (msg_notifier, msg_recv) = smol::channel::unbounded();

        // Listen to msgs from clients
        executor
            .clone()
            .spawn(Self::listen_to_msgs(
                self.p2p.clone(),
                self.model.clone(),
                self.seen.clone(),
                self.unread_events.clone(),
                msg_recv,
                self.clients_subscriptions.clone(),
            ))
            .detach();

        executor
            .clone()
            .spawn(Self::listen_to_view(
                self.view.clone(),
                self.seen.clone(),
                self.missed_events.clone(),
                self.clients_subscriptions.clone(),
            ))
            .detach();

        // Start listening for new connections
        self.listen(msg_notifier, executor.clone()).await?;

        Ok(())
    }

    async fn listen_to_view(
        view: ViewPtr,
        seen: SeenPtr<EventId>,
        missed_events: Arc<Mutex<Vec<Event>>>,
        clients_subscriptions: SubscriberPtr<ClientSubMsg>,
    ) -> Result<()> {
        loop {
            let event = view.lock().await.process().await?;
            if !seen.push(&event.hash()).await {
                continue
            }

            missed_events.lock().await.push(event.clone());

            let msg = match event.action {
                EventAction::PrivMsg(x) => x,
            };
            clients_subscriptions.notify(ClientSubMsg::Privmsg(msg)).await;
        }
    }

    /// Start listening to msgs from irc clients
    pub async fn listen_to_msgs(
        p2p: P2pPtr,
        model: ModelPtr,
        seen: SeenPtr<EventId>,
        unread_events: UnreadEventsPtr,
        recv: smol::channel::Receiver<(NotifierMsg, u64)>,
        clients_subscriptions: SubscriberPtr<ClientSubMsg>,
    ) -> Result<()> {
        loop {
            let (msg, subscription_id) = recv.recv().await?;

            let prev = model.lock().await.get_head_hash();
            match msg {
                NotifierMsg::Privmsg(msg) => {
                    let event = Event {
                        previous_event_hash: prev,
                        action: EventAction::PrivMsg(msg.clone()),
                        timestamp: get_current_time(),
                        read_confirms: 0,
                    };

                    // Since this will be added to the View directly, other clients connected to irc
                    // server must get informed about this new msg
                    clients_subscriptions
                        .notify_with_exclude(ClientSubMsg::Privmsg(msg), &[subscription_id])
                        .await;

                    if !seen.push(&event.hash()).await {
                        continue
                    }
                    unread_events.lock().await.insert(&event);

                    p2p.broadcast(event).await?;
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
        notifier: smol::channel::Sender<(NotifierMsg, u64)>,
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
        notifier: smol::channel::Sender<(NotifierMsg, u64)>,
        executor: Arc<smol::Executor<'_>>,
    ) -> Result<()> {
        let (reader, writer) = stream.split();
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
        executor
            .spawn(async move {
                client.listen().await;
            })
            .detach();

        Ok(())
    }

    /// Setup a listener for irc server
    async fn setup_listener(&self) -> Result<(TcpListener, Option<TlsAcceptor>)> {
        let listenaddr = self.settings.irc_listen.socket_addrs(|| None)?[0];
        let listener = TcpListener::bind(listenaddr).await?;

        let acceptor = match self.settings.irc_listen.scheme() {
            "tls" => {
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
