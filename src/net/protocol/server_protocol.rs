use async_dup::Arc;
use log::*;
use smol::{Async, Executor};
use std::net::{SocketAddr, TcpListener};

//use super::protocol;
use crate::error::Result;
use crate::net::net;
use crate::net::protocol::protocol_base;
use crate::utility::{AddrsStorage, ConnectionsMap};

pub struct ServerProtocol {
    send_sx: async_channel::Sender<net::Message>,
    send_rx: async_channel::Receiver<net::Message>,
    connections: ConnectionsMap,
}

impl ServerProtocol {
    pub fn new(connections: ConnectionsMap) -> Self {
        let (send_sx, send_rx) = async_channel::unbounded::<net::Message>();
        Self {
            send_sx,
            send_rx,
            connections,
        }
    }

    pub fn get_send_pipe(&self) -> async_channel::Sender<net::Message> {
        self.send_sx.clone()
    }

    pub async fn start(
        &mut self,
        address: SocketAddr,
        stored_addrs: AddrsStorage,
        executor: async_dup::Arc<Executor<'_>>,
    ) -> Result<()> {
        let listener = Async::<TcpListener>::bind(address)?;
        info!("Listening on {}", listener.get_ref().local_addr()?);

        loop {
            let (stream, peer_addr) = listener.accept().await?;
            info!("Accepted client: {}", peer_addr);
            let stream = Arc::new(stream);

            let (send_sx, send_rx) = (self.send_sx.clone(), self.send_rx.clone());

            let connections = self.connections.clone();
            connections.lock().await.insert(peer_addr, send_sx.clone());

            let stored_addrs = stored_addrs.clone();
            let executor2 = executor.clone();

            executor
                .spawn(async move {
                    match Self::event_loop_process(
                        stream,
                        stored_addrs,
                        (send_sx, send_rx),
                        connections.clone(),
                        executor2,
                    )
                    .await
                    {
                        Ok(()) => {
                            warn!("Peer {} timeout", peer_addr);
                        }
                        Err(err) => {
                            warn!("Peer {} disconnected: {}", peer_addr, err);
                        }
                    }
                    connections.lock().await.remove(&peer_addr);
                })
                .detach();
        }
    }

    pub async fn event_loop_process(
        mut stream: net::AsyncTcpStream,
        stored_addrs: AddrsStorage,
        (send_sx, send_rx): (
            async_channel::Sender<net::Message>,
            async_channel::Receiver<net::Message>,
        ),
        connections: ConnectionsMap,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        let inactivity_timer = net::InactivityTimer::new(executor.clone());

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
                        None,
                        connections.clone(),
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
