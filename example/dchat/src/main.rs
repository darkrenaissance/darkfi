use async_executor::Executor;
use async_std::sync::{Arc, Mutex};
use easy_parallel::Parallel;

use std::{error, fs::File, io::stdin};

use log::debug;
use simplelog::WriteLogger;
use url::Url;

use darkfi::{net, net::Settings, rpc::server::listen_and_serve};

use crate::{
    dchat_error::ErrorMissingSpecifier,
    dchatmsg::{DchatMsg, DchatMsgsBuffer},
    protocol_dchat::ProtocolDchat,
    rpc::JsonRpcInterface,
};

pub mod dchat_error;
pub mod dchatmsg;
pub mod protocol_dchat;
pub mod rpc;

pub type Error = Box<dyn error::Error>;
pub type Result<T> = std::result::Result<T, Error>;

struct Dchat {
    p2p: net::P2pPtr,
    recv_msgs: DchatMsgsBuffer,
}

impl Dchat {
    fn new(p2p: net::P2pPtr, recv_msgs: DchatMsgsBuffer) -> Self {
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

    async fn register_protocol(&self, msgs: DchatMsgsBuffer) -> Result<()> {
        debug!(target: "dchat", "Dchat::register_protocol() [START]");
        let registry = self.p2p.protocol_registry();
        registry
            .register(!net::SESSION_SEED, move |channel, _p2p| {
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

        self.p2p.stop().await;

        debug!(target: "dchat", "Dchat::start() [STOP]");
        Ok(())
    }

    async fn send(&self, msg: String) -> Result<()> {
        let dchatmsg = DchatMsg { msg };
        self.p2p.broadcast(dchatmsg).await?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct AppSettings {
    accept_addr: Url,
    net: Settings,
}

impl AppSettings {
    pub fn new(accept_addr: Url, net: Settings) -> Self {
        Self { accept_addr, net }
    }
}

fn alice() -> Result<AppSettings> {
    let log_level = simplelog::LevelFilter::Debug;
    let log_config = simplelog::Config::default();

    let log_path = "/tmp/alice.log";
    let file = File::create(log_path).unwrap();
    WriteLogger::init(log_level, log_config, file)?;

    let seed = Url::parse("tcp://127.0.0.1:50515").unwrap();
    let inbound = Url::parse("tcp://127.0.0.1:51554").unwrap();
    let ext_addr = Url::parse("tcp://127.0.0.1:51554").unwrap();

    let net = Settings {
        inbound: Some(inbound),
        external_addr: Some(ext_addr),
        seeds: vec![seed],
        ..Default::default()
    };

    let accept_addr = Url::parse("tcp://127.0.0.1:55054").unwrap();
    let settings = AppSettings::new(accept_addr, net);

    Ok(settings)
}

fn bob() -> Result<AppSettings> {
    let log_level = simplelog::LevelFilter::Debug;
    let log_config = simplelog::Config::default();

    let log_path = "/tmp/bob.log";
    let file = File::create(log_path).unwrap();
    WriteLogger::init(log_level, log_config, file)?;

    let seed = Url::parse("tcp://127.0.0.1:50515").unwrap();

    let net = Settings {
        inbound: None,
        outbound_connections: 5,
        seeds: vec![seed],
        ..Default::default()
    };

    let accept_addr = Url::parse("tcp://127.0.0.1:51054").unwrap();
    let settings = AppSettings::new(accept_addr, net);

    Ok(settings)
}

#[async_std::main]
async fn main() -> Result<()> {
    let settings: Result<AppSettings> = match std::env::args().nth(1) {
        Some(id) => match id.as_str() {
            "a" => alice(),
            "b" => bob(),
            _ => Err(ErrorMissingSpecifier.into()),
        },
        None => Err(ErrorMissingSpecifier.into()),
    };

    let settings = settings?.clone();

    let p2p = net::P2p::new(settings.net.into()).await;

    let nthreads = num_cpus::get();
    let (signal, shutdown) = async_channel::unbounded::<()>();

    let ex = Arc::new(Executor::new());
    let ex2 = ex.clone();
    let ex3 = ex2.clone();

    let msgs: DchatMsgsBuffer = Arc::new(Mutex::new(vec![DchatMsg { msg: String::new() }]));

    let mut dchat = Dchat::new(p2p.clone(), msgs);

    let accept_addr = settings.accept_addr.clone();
    let rpc = Arc::new(JsonRpcInterface { addr: accept_addr.clone(), p2p });
    ex.spawn(async move { listen_and_serve(accept_addr.clone(), rpc).await }).detach();

    let (_, result) = Parallel::new()
        .each(0..nthreads, |_| smol::future::block_on(ex2.run(shutdown.recv())))
        .finish(|| {
            smol::future::block_on(async move {
                dchat.start(ex3).await?;
                drop(signal);
                Ok(())
            })
        });

    result
}
