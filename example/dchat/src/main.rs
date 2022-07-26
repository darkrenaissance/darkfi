use async_executor::Executor;
use async_std::sync::Arc;
use easy_parallel::Parallel;
use log::{error, info};
use simplelog::WriteLogger;
use std::{
    fs::File,
    io::{self, Write},
};
use termion::{event::Key, input::TermRead};
use url::Url;

use darkfi::{
    net,
    net::Settings,
    util::cli::{get_log_config, get_log_level},
    Result,
};

use crate::{dchatmsg::Dchatmsg, protocol_dchat::ProtocolDchat};

pub mod dchatmsg;
pub mod protocol_dchat;

struct Dchat {
    p2p: net::P2pPtr,
}

impl Dchat {
    fn new(p2p: net::P2pPtr) -> Self {
        Self { p2p }
    }

    async fn render(&self, ex: Arc<Executor<'_>>) -> Result<()> {
        info!("DCHAT::render()::start");
        let mut stdout = io::stdout().lock();
        let stdin = termion::async_stdin();
        let mut keys = stdin.keys();

        stdout.write_all(
            b"Welcome to dchat
    s: send message
    i. inbox
    q: quit \n",
        )?;

        ex.spawn(async move {
            loop {
                let k = keys.next();
                match k {
                    Some(k) => match k {
                        Ok(k) => match k {
                            Key::Char('q') => {
                                info!("DCHAT::Q pressed.... exiting");
                                break
                            }
                            Key::Char('i') => {}
                            Key::Char('s') => {
                                // TODO
                                //self.send().await?;
                            }
                            _ => {}
                        },
                        Err(e) => {
                            error!("found error: {}", e);
                        }
                    },
                    None => {}
                }
            }
        })
        .detach();
        Ok(())
    }

    async fn register_protocol(&self) -> Result<()> {
        info!("DCHAT::register_protocol()::start");
        let registry = self.p2p.protocol_registry();
        registry
            .register(net::SESSION_ALL, move |channel, p2p| async move {
                ProtocolDchat::init(channel, p2p).await
            })
            .await;
        info!("DCHAT::register_protocol()::stop");
        Ok(())
    }

    async fn start(&self, executor: Arc<Executor<'_>>) -> Result<()> {
        info!("DCHAT::start()::start");
        let dchat = Dchat::new(self.p2p.clone());

        dchat.register_protocol().await?;

        self.p2p.clone().start(executor.clone()).await?;
        let executor_cloned = executor.clone();
        executor_cloned.spawn(self.p2p.clone().run(executor.clone())).detach();

        let result = dchat.render(executor.clone()).await;

        if let Err(e) = result {
            error!("Rendering failed {}", e);
        };

        info!("DCHAT::start()::stop");
        Ok(())
    }

    async fn _send(&self) -> Result<()> {
        let dchatmsg = Dchatmsg { message: "helloworld".to_string() };
        self.p2p.broadcast(dchatmsg).await?;
        Ok(())
    }
}

#[async_std::main]
async fn main() -> Result<()> {
    let log_level = get_log_level(1);
    let log_config = get_log_config();

    let log_path = "/tmp/dchat.log";
    let file = File::create(log_path).unwrap();
    WriteLogger::init(log_level, log_config, file)?;

    let seed = Url::parse("tcp://127.0.0.1:55555").unwrap();
    let inbound = Url::parse("tcp://127.0.0.1:55554").unwrap();
    let ext_addr = Url::parse("tcp://127.0.0.1:55544").unwrap();

    let settings = Settings {
        inbound: Some(inbound),
        outbound_connections: 0,
        manual_attempt_limit: 0,
        seed_query_timeout_seconds: 8,
        connect_timeout_seconds: 10,
        channel_handshake_seconds: 4,
        channel_heartbeat_seconds: 10,
        outbound_retry_seconds: 1200,
        external_addr: Some(ext_addr),
        peers: Vec::new(),
        seeds: vec![seed],
        node_id: String::new(),
    };

    let p2p = net::P2p::new(settings).await;
    let p2p = p2p.clone();

    let nthreads = num_cpus::get();
    let (signal, shutdown) = async_channel::unbounded::<()>();

    let ex = Arc::new(Executor::new());
    let ex2 = ex.clone();

    let dchat = Dchat::new(p2p.clone());

    let (_, result) = Parallel::new()
        .each(0..nthreads, |_| smol::future::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::future::block_on(async move {
                dchat.start(ex2).await?;
                drop(signal);
                Ok(())
            })
        });

    result
}
