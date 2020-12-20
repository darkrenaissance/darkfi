use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::AtomicU64;
use log::*;
use smol::{Async, Executor};
use async_dup::Arc;
use rand::seq::SliceRandom;

use crate::error::Result;
use crate::net::net;
use crate::net::protocol::protocol_base;
use crate::utility::{AddrsStorage, ConnectionsMap};

pub struct ClientProtocol {
    send_sx: async_channel::Sender<net::Message>,
    send_rx: async_channel::Receiver<net::Message>,
    connections: ConnectionsMap,
    main_process: Option<smol::Task<()>>,
}

impl ClientProtocol {
    pub fn new(connections: ConnectionsMap) -> Self {
        let (send_sx, send_rx) = async_channel::unbounded::<net::Message>();
        Self {
            send_sx,
            send_rx,
            connections,
            main_process: None
        }
    }

    pub fn get_send_pipe(&self) -> async_channel::Sender<net::Message> {
        self.send_sx.clone()
    }

    async fn fetch_random_addr(
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
                    if addr == *accept_addr { continue; }
                }
        }

    }

    pub async fn start(
        &mut self,
        accept_addr: Option<SocketAddr>,
        stored_addrs: AddrsStorage,
        executor: Arc<Executor<'_>>
    ) {
        let connections = self.connections.clone();
        let (send_sx, send_rx) = (self.send_sx.clone(), self.send_rx.clone());

        let executor2 = executor.clone();

        self.main_process = Some(executor.spawn(async move {
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
                    if addr == accept_addr { continue; }
                }

                debug!("Attempting connect to {}", addr);

                Self::try_connect_process(addr, connections.clone(), accept_addr.clone(), stored_addrs.clone(), (send_sx.clone(), send_rx.clone()), executor2.clone()).await;

                // TODO: Fix this
                net::sleep(2).await;
            }
        }));
    }

    pub async fn start_manual(
        &mut self,
        remote_addr: SocketAddr,
        accept_addr: Option<SocketAddr>,
        stored_addrs: AddrsStorage,
        executor: Arc<Executor<'_>>
        ) {
        let connections = self.connections.clone();
        let (send_sx, send_rx) = (self.send_sx.clone(), self.send_rx.clone());

        let executor2 = executor.clone();

        self.main_process = Some(executor.spawn(async move {
            loop {
                for _ in 0..4 {
                    debug!("Attempting connect to {}", remote_addr);

                    Self::try_connect_process(remote_addr, connections.clone(), accept_addr.clone(), stored_addrs.clone(), (send_sx.clone(), send_rx.clone()), executor2.clone()).await;
                }
                net::sleep(2).await;
            }
        }));
    }

    pub async fn try_connect_process(
        address: SocketAddr,
        connections: ConnectionsMap,
        accept_addr: Option<SocketAddr>,
        stored_addrs: AddrsStorage,
        (send_sx, send_rx): (
            async_channel::Sender<net::Message>,
            async_channel::Receiver<net::Message>,
        ),
        executor: Arc<Executor<'_>>
    ) {
        match Async::<TcpStream>::connect(address.clone()).await {
            Ok(stream) => {
                let _ = Self::handle_connect(
                    stream,
                    stored_addrs.clone(),
                    connections,
                    address,
                    (send_sx.clone(), send_rx.clone()),
                    accept_addr,
                    executor
                )
                .await;
            }
            Err(_err) => {  warn!(
                              "Unable to connect to addr {:?}: {}",
                             address, _err
                             ); }
        }
    }

    async fn handle_connect(
        stream: Async<TcpStream>,
        stored_addrs: AddrsStorage,
        connections: ConnectionsMap,
        address: SocketAddr,
        (send_sx, send_rx): (
            async_channel::Sender<net::Message>,
            async_channel::Receiver<net::Message>,
        ),
        accept_addr: Option<SocketAddr>,
        executor: Arc<Executor<'_>>
    ) -> Result<()> {
        debug!("Connected to {}", address);

        let stream = async_dup::Arc::new(stream);
        connections
            .lock()
            .await
            .insert(address.clone(), send_sx.clone());

        // Run event loop
        match Self::event_loop_process(
            stream,
            stored_addrs,
            (send_sx, send_rx),
            accept_addr,
            connections.clone(),
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
        connections.lock().await.remove(&address);
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
        mut stream: net::AsyncTcpStream,
        stored_addrs: AddrsStorage,
        (send_sx, send_rx): (
            async_channel::Sender<net::Message>,
            async_channel::Receiver<net::Message>,
        ),
        accept_addr: Option<SocketAddr>,
        connections: ConnectionsMap,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        let inactivity_timer = net::InactivityTimer::new(executor.clone());

        let clock = Arc::new(AtomicU64::new(0));
        let send_sx2 = send_sx.clone();
        let clock2 = clock.clone();
        let ping_task = executor.spawn(
            protocol_base::repeat_ping(send_sx2, clock2)
        );

        let mut send_addr_task = None;
        if let Some(accept_addr) = accept_addr {
            send_addr_task = Some(executor.spawn(Self::send_addr(send_sx.clone(), accept_addr)));
        }

        loop {
            let event = net::select_event(&mut stream, &send_rx, &inactivity_timer).await?;

            match event {
                net::Event::Send(message) => {
                    net::send_message(&mut stream, message).await?;
                }
                net::Event::Receive(message) => {
                    inactivity_timer.reset().await?;
                    protocol_base::protocol(
                        message,
                        &stored_addrs,
                        &send_sx,
                        Some(&clock),
                        connections.clone(),
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
