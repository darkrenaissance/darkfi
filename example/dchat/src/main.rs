use async_executor::Executor;
use async_std::sync::{Arc, Mutex};
use easy_parallel::Parallel;

use std::{fs::File, io::stdin};

use log::debug;
use simplelog::WriteLogger;
use url::Url;

use darkfi::{net, net::Settings};

use crate::{
    dchatmsg::{Dchatmsg, DchatmsgsBuffer},
    error::{MissingSpecifier, Result},
    protocol_dchat::ProtocolDchat,
};

pub mod dchatmsg;
pub mod error;
pub mod protocol_dchat;

struct Dchat {
    p2p: net::P2pPtr,
    recv_msgs: DchatmsgsBuffer,
}

impl Dchat {
    fn new(p2p: net::P2pPtr, recv_msgs: DchatmsgsBuffer) -> Self {
        Self { p2p, recv_msgs }
    }

    async fn menu(&self) -> Result<()> {
        let mut buffer = String::new();
        let stdin = stdin();
        loop {
            println!(
                "Welcome to dchat.
    s: send message
    i: inbox
    q: quit "
            );
            stdin.read_line(&mut buffer)?;
            // Remove trailing \n
            buffer.pop();
            match buffer.as_str() {
                "q" => return Ok(()),
                "s" => {
                    // Remove trailing s
                    buffer.pop();
                    stdin.read_line(&mut buffer)?;
                    match self.send(buffer.clone()).await {
                        Ok(_) => {
                            println!("you sent: {}", buffer);
                        }
                        Err(e) => {
                            println!("send failed for reason: {}", e);
                        }
                    }
                    buffer.clear();
                }
                "i" => {
                    let msgs = self.recv_msgs.lock().await;
                    if msgs.is_empty() {
                        println!("inbox is empty")
                    } else {
                        println!("received:");
                        for i in msgs.iter() {
                            if !i.msg.is_empty() {
                                println!("{}", i.msg);
                            }
                        }
                    }
                    buffer.clear();
                }
                _ => {}
            }
        }
    }

    async fn register_protocol(&self, msgs: DchatmsgsBuffer) -> Result<()> {
        debug!(target: "dchat", "Dchat::register_protocol() [START]");
        let registry = self.p2p.protocol_registry();
        registry
            .register(net::SESSION_ALL, move |channel, _p2p| {
                let msgs2 = msgs.clone();
                async move { ProtocolDchat::init(channel, msgs2).await }
            })
            .await;
        debug!(target: "dchat", "Dchat::register_protocol() [STOP]");
        Ok(())
    }

    async fn start(&mut self, ex: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "dchat", "Dchat::start() [START]");

        let ex2 = ex.clone();

        self.register_protocol(self.recv_msgs.clone()).await?;
        self.p2p.clone().start(ex.clone()).await?;
        ex2.spawn(self.p2p.clone().run(ex.clone())).detach();

        self.menu().await?;

        debug!(target: "dchat", "Dchat::start() [STOP]");
        Ok(())
    }

    async fn send(&self, msg: String) -> Result<()> {
        //let msg = self.input.clone();
        let dchatmsg = Dchatmsg { msg };
        self.p2p.broadcast(dchatmsg).await?;
        Ok(())
    }
}

// inbound
fn alice() -> Result<Settings> {
    let log_level = simplelog::LevelFilter::Debug;
    let log_config = simplelog::Config::default();

    let log_path = "/tmp/alice.log";
    let file = File::create(log_path).unwrap();
    WriteLogger::init(log_level, log_config, file)?;

    let seed = Url::parse("tcp://127.0.0.1:55555").unwrap();
    let inbound = Url::parse("tcp://127.0.0.1:55554").unwrap();
    let ext_addr = Url::parse("tcp://127.0.0.1:55554").unwrap();

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

    Ok(settings)
}

// outbound
fn bob() -> Result<Settings> {
    let log_level = simplelog::LevelFilter::Debug;
    let log_config = simplelog::Config::default();

    let log_path = "/tmp/bob.log";
    let file = File::create(log_path).unwrap();
    WriteLogger::init(log_level, log_config, file)?;

    let seed = Url::parse("tcp://127.0.0.1:55555").unwrap();
    let oc = 5;

    let settings = Settings {
        inbound: None,
        outbound_connections: oc,
        manual_attempt_limit: 0,
        seed_query_timeout_seconds: 8,
        connect_timeout_seconds: 10,
        channel_handshake_seconds: 4,
        channel_heartbeat_seconds: 10,
        outbound_retry_seconds: 1200,
        external_addr: None,
        peers: Vec::new(),
        seeds: vec![seed],
        node_id: String::new(),
    };

    Ok(settings)
}

#[async_std::main]
async fn main() -> Result<()> {
    let settings: Result<Settings> = match std::env::args().nth(1) {
        Some(id) => match id.as_str() {
            "a" => alice(),
            "b" => bob(),
            _ => Err(MissingSpecifier.into()),
        },
        None => Err(MissingSpecifier.into()),
    };

    let p2p = net::P2p::new(settings?.into()).await;

    let nthreads = num_cpus::get();
    let (signal, shutdown) = async_channel::unbounded::<()>();

    let ex = Arc::new(Executor::new());
    let ex2 = ex.clone();

    let msgs: DchatmsgsBuffer = Arc::new(Mutex::new(vec![Dchatmsg { msg: String::new() }]));

    let mut dchat = Dchat::new(p2p, msgs);

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
