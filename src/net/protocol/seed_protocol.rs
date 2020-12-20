use async_dup::Arc;
use log::*;
use smol::{Async, Executor};
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::Result;
use crate::net::net;
use crate::net::protocol::protocol_base;
use crate::utility::{get_current_time, AddrsStorage};

type Clock = Arc<AtomicU64>;

pub struct SeedProtocol {
    send_sx: async_channel::Sender<net::Message>,
    send_rx: async_channel::Receiver<net::Message>,
    main_process: Option<smol::Task<()>>,
}

#[derive(PartialEq)]
enum ProtocolSignal {
    Waiting,
    Finished,
    Timeout
}

impl SeedProtocol {
    pub fn new() -> Self {
        let (send_sx, send_rx) = async_channel::unbounded::<net::Message>();
        Self {
            send_sx,
            send_rx,
            main_process: None,
        }
    }

    pub async fn start(
        &mut self,
        seed_addr: SocketAddr,
        local_addr: Option<SocketAddr>,
        stored_addrs: AddrsStorage,
        executor: Arc<Executor<'_>>,
    ) {
        let (send_sx, send_rx) = (self.send_sx.clone(), self.send_rx.clone());
        let ex = executor.clone();
        self.main_process = Some(ex.spawn(async move {
            match Async::<TcpStream>::connect(seed_addr.clone()).await {
                Ok(stream) => {
                    let _ = Self::handle_connect(
                        stream,
                        &stored_addrs,
                        seed_addr.clone(),
                        local_addr,
                        (send_sx.clone(), send_rx.clone()),
                        executor.clone(),
                    )
                    .await;
                }
                Err(err) => { warn!("Unable to connect to seed {}: {}", seed_addr, err) },
            }
        }));
    }

    pub async fn await_finish(self) {
        if let Some(process) = self.main_process {
            process.await;
        }
    }

    async fn handle_connect(
        stream: Async<TcpStream>,
        stored_addrs: &AddrsStorage,
        seed_addr: SocketAddr,
        local_addr: Option<SocketAddr>,
        (send_sx, send_rx): (
            async_channel::Sender<net::Message>,
            async_channel::Receiver<net::Message>,
        ),
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        if let Some(local_addr) = local_addr {
            send_sx
                .send(net::Message::Addrs(net::AddrsMessage { addrs: vec![local_addr] }))
                .await?;
        }

        send_sx
            .send(net::Message::GetAddrs(net::GetAddrsMessage {}))
            .await?;

        let stream = Arc::new(stream);

        // Run event loop
        match Self::event_loop_process(
            stream,
            stored_addrs.clone(),
            (send_sx, send_rx),
            executor,
        )
        .await
        {
            Ok(ProtocolSignal::Finished) => {
                info!("Seed node queried successfully: {}", seed_addr);
            }
            Ok(ProtocolSignal::Timeout) => {
                warn!("Seed node timeout: {}", seed_addr);
            }
            Ok(_) => { unreachable!(); }
            Err(err) => {
                warn!("Seed disconnected: {} {}", seed_addr, err);
            }
        }
        Ok(())
    }

    async fn event_loop_process(
        mut stream: net::AsyncTcpStream,
        stored_addrs: AddrsStorage,
        (send_sx, send_rx): (
            async_channel::Sender<net::Message>,
            async_channel::Receiver<net::Message>,
        ),
        executor: Arc<Executor<'_>>,
    ) -> Result<ProtocolSignal> {
        let inactivity_timer = net::InactivityTimer::new(executor.clone());

        let clock = Arc::new(AtomicU64::new(0));
        let _ping_task = executor.spawn(protocol_base::repeat_ping(send_sx.clone(), clock.clone()));

        loop {
            let event = net::select_event(&mut stream, &send_rx, &inactivity_timer).await?;

            match event {
                net::Event::Send(message) => {
                    net::send_message(&mut stream, message).await?;
                }
                net::Event::Receive(message) => {
                    inactivity_timer.reset().await?;
                    let signal = Self::protocol(message, &stored_addrs, &send_sx, &clock).await?;

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

    async fn protocol(
        message: net::Message,
        stored_addrs: &AddrsStorage,
        _send_sx: &async_channel::Sender<net::Message>,
        clock: &Clock,
    ) -> Result<ProtocolSignal> {
        match message {
            net::Message::Pong => {
                let current_time = get_current_time();
                let elapsed = current_time - clock.load(Ordering::Relaxed);
                info!("Ping time: {} ms", elapsed);
            }
            net::Message::Addrs(message) => {
                info!("received AddrMessage");
                let mut stored_addrs = stored_addrs.lock().await;
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
