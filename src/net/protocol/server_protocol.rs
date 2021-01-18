use log::*;
use smol::{Async, Executor};
use std::net::{SocketAddr, TcpListener};
use std::sync::Arc;

//use super::protocol;
use crate::error::Result;
use crate::net::messages as net;
use crate::net::protocol::protocol_base;
use crate::utility::{AddrsStorage, ConnectionsMap};

pub struct ServerProtocol {
    send_sx: async_channel::Sender<net::Message>,
    send_rx: async_channel::Receiver<net::Message>,
    connections: ConnectionsMap,

    accept_addr: SocketAddr,
    stored_addrs: AddrsStorage,
}

impl ServerProtocol {
    pub fn new(
        connections: ConnectionsMap,
        accept_addr: SocketAddr,
        stored_addrs: AddrsStorage,
    ) -> Arc<Self> {
        let (send_sx, send_rx) = async_channel::unbounded::<net::Message>();
        Arc::new(Self {
            send_sx,
            send_rx,
            connections,

            accept_addr,
            stored_addrs,
        })
    }

    pub fn get_send_pipe(&self) -> async_channel::Sender<net::Message> {
        self.send_sx.clone()
    }

    pub async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        let listener = Async::<TcpListener>::bind(self.accept_addr)?;
        info!("Listening on {}", listener.get_ref().local_addr()?);

        loop {
            let (stream, peer_addr) = listener.accept().await?;
            info!("Accepted client: {}", peer_addr);
            let stream = async_dup::Arc::new(stream);

            self.connections
                .lock()
                .await
                .insert(peer_addr, self.send_sx.clone());

            let executor2 = executor.clone();
            let self2 = self.clone();

            executor
                .spawn(async move {
                    match self2.clone().event_loop_process(stream, executor2).await {
                        Ok(()) => {
                            warn!("Peer {} timeout", peer_addr);
                        }
                        Err(err) => {
                            warn!("Peer {} disconnected: {}", peer_addr, err);
                        }
                    }
                    self2.connections.lock().await.remove(&peer_addr);
                })
                .detach();
        }
    }

    pub async fn event_loop_process(
        self: Arc<Self>,
        mut stream: net::AsyncTcpStream,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        let inactivity_timer = net::InactivityTimer::new(executor.clone());

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
                        None,
                        self.connections.clone(),
                    )
                    .await?;
                }
                net::Event::Timeout => break,
            }
        }

        inactivity_timer.stop().await;
        // Connection timed out
        Ok(())
    }
}
