use async_std::sync::Mutex;
use log::*;
use smol::{Async, Executor};
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::error::Result;
use crate::net::messages as net;
use crate::net::protocol::protocol_base;
use crate::utility::{get_current_time, AddrsStorage};

type Clock = Arc<AtomicU64>;

pub struct SeedProtocol {
    send_sx: async_channel::Sender<net::Message>,
    send_rx: async_channel::Receiver<net::Message>,
    main_process: Mutex<Option<smol::Task<()>>>,

    seed_addr: SocketAddr,
    accept_addr: Option<SocketAddr>,
    stored_addrs: AddrsStorage,
}

#[derive(PartialEq)]
enum ProtocolSignal {
    Waiting,
    Finished,
    Timeout,
}

impl SeedProtocol {
    pub fn new(
        seed_addr: SocketAddr,
        accept_addr: Option<SocketAddr>,
        stored_addrs: AddrsStorage,
    ) -> Arc<Self> {
        let (send_sx, send_rx) = async_channel::unbounded::<net::Message>();
        Arc::new(Self {
            send_sx,
            send_rx,
            main_process: Mutex::new(None),
            seed_addr,
            accept_addr,
            stored_addrs,
        })
    }

    pub async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) {
        let executor2 = executor.clone();
        let self2 = self.clone();

        *self2.main_process.lock().await = Some(executor.spawn(async move {
            match Async::<TcpStream>::connect(self.seed_addr).await {
                Ok(stream) => {
                    let _ = self.handle_connect(stream, executor2).await;
                }
                Err(err) => {
                    warn!("Unable to connect to seed {}: {}", self.seed_addr, err)
                }
            }
        }));
    }

    pub async fn await_finish(self: Arc<Self>) {
        let mut process = self.main_process.lock().await;
        if let Some(process) = &mut *process {
            process.await;
        }
    }

    async fn handle_connect(
        &self,
        stream: Async<TcpStream>,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        if let Some(accept_addr) = self.accept_addr {
            self.send_sx
                .send(net::Message::Addrs(net::AddrsMessage {
                    addrs: vec![accept_addr],
                }))
                .await?;
        }

        self.send_sx
            .send(net::Message::GetAddrs(net::GetAddrsMessage {}))
            .await?;

        let stream = async_dup::Arc::new(stream);

        // Run event loop
        match self.event_loop_process(stream, executor).await {
            Ok(ProtocolSignal::Finished) => {
                info!("Seed node queried successfully: {}", self.seed_addr);
            }
            Ok(ProtocolSignal::Timeout) => {
                warn!("Seed node timeout: {}", self.seed_addr);
            }
            Ok(_) => {
                unreachable!();
            }
            Err(err) => {
                warn!("Seed disconnected: {} {}", self.seed_addr, err);
            }
        }
        Ok(())
    }

    async fn event_loop_process(
        &self,
        mut stream: net::AsyncTcpStream,
        executor: Arc<Executor<'_>>,
    ) -> Result<ProtocolSignal> {
        let inactivity_timer = net::InactivityTimer::new(executor.clone());

        let clock = Arc::new(AtomicU64::new(0));
        let _ping_task = executor.spawn(protocol_base::repeat_ping(
            self.send_sx.clone(),
            clock.clone(),
        ));

        loop {
            let event = net::select_event(&mut stream, &self.send_rx, &inactivity_timer).await?;

            match event {
                net::Event::Send(message) => {
                    net::send_message(&mut stream, message).await?;
                }
                net::Event::Receive(message) => {
                    inactivity_timer.reset().await?;
                    let signal = self.protocol(message, &clock).await?;

                    if signal == ProtocolSignal::Finished {
                        return Ok(ProtocolSignal::Finished);
                    }
                }
                net::Event::Timeout => return Ok(ProtocolSignal::Timeout),
            }
        }

        // These aren't needed since drop() cancels tasks anyway
        //ping_task.cancel().await;
        //inactivity_timer.stop().await;
    }

    async fn protocol(&self, message: net::Message, clock: &Clock) -> Result<ProtocolSignal> {
        match message {
            net::Message::Pong => {
                let current_time = get_current_time();
                let elapsed = current_time - clock.load(Ordering::Relaxed);
                info!("Ping time: {} ms", elapsed);
            }
            net::Message::Addrs(message) => {
                info!("received AddrMessage");
                let mut stored_addrs = self.stored_addrs.lock().await;
                for addr in message.addrs {
                    if !stored_addrs.contains(&addr) {
                        stored_addrs.push(addr);
                        info!("Added new address to storage {}", addr.to_string());
                    }
                }

                return Ok(ProtocolSignal::Finished);
            }
            _ => {}
        }

        Ok(ProtocolSignal::Waiting)
    }
}
