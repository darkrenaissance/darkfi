use async_std::sync::Mutex;
use std::sync::Arc;
use log::*;
use rand::seq::SliceRandom;
use smol::{Async, Executor};
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::AtomicU64;

use crate::error::Result;
use crate::net::net;
use crate::net::protocol::protocol_base;
use crate::utility::{AddrsStorage, ConnectionsMap};

pub struct ClientProtocol {
    send_sx: async_channel::Sender<net::Message>,
    send_rx: async_channel::Receiver<net::Message>,
    connections: ConnectionsMap,
    main_process: Mutex<Option<smol::Task<()>>>,

    accept_addr: Option<SocketAddr>,
    stored_addrs: AddrsStorage,
}

impl ClientProtocol {
    pub fn new(connections: ConnectionsMap, accept_addr: Option<SocketAddr>,
    stored_addrs: AddrsStorage,
               ) -> Arc<Self> {
        let (send_sx, send_rx) = async_channel::unbounded::<net::Message>();
        Arc::new(Self {
            send_sx,
            send_rx,
            connections,
            main_process: Mutex::new(None),
            accept_addr,
            stored_addrs
        })
    }

    pub fn get_send_pipe(&self) -> async_channel::Sender<net::Message> {
        self.send_sx.clone()
    }

    async fn fetch_random_addr(
        self: Arc<Self>,
        accept_addr: &Option<SocketAddr>,
        stored_addrs: &AddrsStorage,
        connections: &ConnectionsMap,
    ) {
        loop {
            let addr = match stored_addrs.lock().await.choose(&mut rand_core::OsRng) {
                Some(addr) => addr.clone(),
                None => {
                    debug!("No addresses in store. Sleeping for 2 secs before retrying...");
                    net::sleep(2).await;
                    continue;
                }
            };
            if connections.lock().await.contains_key(&addr) {
                continue;
            }
            if let Some(accept_addr) = accept_addr {
                if addr == *accept_addr {
                    continue;
                }
            }
        }
    }

    pub async fn start(
        self: Arc<Self>,
        executor: Arc<Executor<'_>>,
    ) {
        let executor2 = executor.clone();
        let self2 = self.clone();

        *self2.main_process.lock().await = Some(executor.spawn(async move {
            loop {
                let addr = match self.stored_addrs.lock().await.choose(&mut rand_core::OsRng) {
                    Some(addr) => addr.clone(),
                    None => {
                        debug!("No addresses in store. Sleeping for 2 secs before retrying...");
                        net::sleep(2).await;
                        continue;
                    }
                };
                if self.connections.lock().await.contains_key(&addr) {
                    continue;
                }
                if let Some(accept_addr) = self.accept_addr {
                    if addr == accept_addr {
                        continue;
                    }
                }

                debug!("Attempting connect to {}", addr);

                self.try_connect_process(
                    addr,
                    executor2.clone(),
                )
                .await;

                // TODO: Fix this
                net::sleep(2).await;
            }
        }));
    }

    pub async fn start_manual(
        self: Arc<Self>,
        remote_addr: SocketAddr,
        executor: Arc<Executor<'_>>,
    ) {
        let executor2 = executor.clone();
        let self2 = self.clone();

        *self2.main_process.lock().await = Some(executor.spawn(async move {
            loop {
                for _ in 0..4 {
                    debug!("Attempting connect to {}", remote_addr);

                    self.try_connect_process(
                        remote_addr,
                        executor2.clone(),
                    )
                    .await;
                }
                net::sleep(2).await;
            }
        }));
    }

    pub async fn try_connect_process(
        &self,
        address: SocketAddr,
        executor: Arc<Executor<'_>>,
    ) {
        match Async::<TcpStream>::connect(address.clone()).await {
            Ok(stream) => {
                let _ = self.handle_connect(
                    stream,
                    address,
                    executor,
                )
                .await;
            }
            Err(_err) => {
                warn!("Unable to connect to addr {:?}: {}", address, _err);
            }
        }
    }

    async fn handle_connect(
        &self,
        stream: Async<TcpStream>,
        address: SocketAddr,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        debug!("Connected to {}", address);

        let stream = async_dup::Arc::new(stream);
        self.connections
            .lock()
            .await
            .insert(address.clone(), self.send_sx.clone());

        // Run event loop
        match self.event_loop_process(
            stream,
            executor,
        )
        .await
        {
            Ok(()) => {
                warn!("Server timeout");
            }
            Err(err) => {
                warn!("Server disconnected: {}", err);
            }
        }
        self.connections.lock().await.remove(&address);
        Ok(())
    }

    async fn send_addr(
        send_sx: async_channel::Sender<net::Message>,
        accept_addr: SocketAddr,
    ) -> Result<()> {
        loop {
            send_sx
                .send(net::Message::Addrs(net::AddrsMessage {
                    addrs: vec![accept_addr],
                }))
                .await?;

            net::sleep(3600).await;
        }
    }

    pub async fn event_loop_process(
        &self,
        mut stream: net::AsyncTcpStream,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        let inactivity_timer = net::InactivityTimer::new(executor.clone());

        let clock = Arc::new(AtomicU64::new(0));
        let send_sx2 = self.send_sx.clone();
        let clock2 = clock.clone();
        let ping_task = executor.spawn(protocol_base::repeat_ping(send_sx2, clock2));

        let mut send_addr_task = None;
        if let Some(accept_addr) = self.accept_addr {
            send_addr_task = Some(executor.spawn(Self::send_addr(self.send_sx.clone(), accept_addr.clone())));
        }

        loop {
            let event = net::select_event(&mut stream, &self.send_rx, &inactivity_timer).await?;

            match event {
                net::Event::Send(message) => {
                    net::send_message(&mut stream, message).await?;
                }
                net::Event::Receive(message) => {
                    inactivity_timer.reset().await?;
                    protocol_base::protocol(
                        message,
                        &self.stored_addrs,
                        &self.send_sx,
                        Some(&clock),
                        self.connections.clone(),
                    )
                    .await?;
                }
                net::Event::Timeout => break,
            }
        }

        if let Some(send_addr_task) = send_addr_task {
            send_addr_task.cancel().await;
        }
        ping_task.cancel().await;
        inactivity_timer.stop().await;

        // Connection timed out
        Ok(())
    }
}
