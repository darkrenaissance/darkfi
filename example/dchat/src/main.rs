use async_executor::Executor;
use async_std::sync::{Arc, Mutex};
use easy_parallel::Parallel;
use log::{error, info};
use simplelog::WriteLogger;
use std::{
    fs::File,
    io::{self, Read, Write},
};
use termion::{async_stdin, event::Key, input::TermRead};
use url::Url;

use darkfi::{
    net,
    net::Settings,
    util::cli::{get_log_config, get_log_level},
    Error, Result,
};

use crate::{
    dchatmsg::{Dchatmsg, DchatmsgsBuffer},
    protocol_dchat::ProtocolDchat,
};

pub mod dchatmsg;
pub mod protocol_dchat;

struct Dchat {
    p2p: net::P2pPtr,
    msgs: DchatmsgsBuffer,
}

impl Dchat {
    fn new(p2p: net::P2pPtr, msgs: DchatmsgsBuffer) -> Arc<Self> {
        Arc::new(Self { p2p, msgs })
    }

    async fn render(&self) -> Result<()> {
        info!(target: "dchat", "DCHAT::render()::start");
        let mut stdout = io::stdout().lock();
        let mut stdin = async_stdin();

        println!(
            "Welcome to dchat
                s: send message
                i. inbox
                q: quit \n",
        );

        loop {
            for k in stdin.by_ref().keys() {
                match k.unwrap() {
                    Key::Char('q') => {
                        info!(target: "dchat", "DCHAT::Q pressed.... exiting");
                        return Ok(())
                    }
                    Key::Char('i') => {
                        let vec = self.msgs.lock().await;
                        for i in vec.iter() {
                            println!("iterated version {:?}", i);
                        }
                        println!("with indexing {:?}", vec[0]);
                        //for v in vec {
                        //    //
                        //}
                        //for msg in self.msgs.lock().await {
                        //    //println!("{}", msg);
                        //}
                    }

                    Key::Char('s') => {
                        //stdout.write_all(b"type your message and then press enter\n")?;
                        //let mut input = String::new();
                        //stdin.read_line(&mut input)?;

                        let msg = self.get_input().await?;
                        self.send(msg).await?;
                    }
                    _ => {}
                }
            }
        }
    }

    async fn get_input(&self) -> Result<String> {
        let mut stdout = io::stdout().lock();
        stdout.write_all(b"type your message and then press enter\n")?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        stdout.write_all(b"you entered:")?;
        stdout.write_all(input.as_bytes())?;
        return Ok(input)
    }

    async fn register_protocol(&self, msgs: DchatmsgsBuffer) -> Result<()> {
        info!(target: "dchat", "dchat::register_protocol()::start");
        let registry = self.p2p.protocol_registry();
        registry
            .register(net::SESSION_ALL, move |channel, p2p| {
                let msgs2 = msgs.clone();
                async move { ProtocolDchat::init(channel, p2p, msgs2).await }
            })
            .await;
        info!(target: "dchat", "DCHAT::register_protocol()::stop");
        Ok(())
    }

    async fn start(&self, ex: Arc<Executor<'_>>) -> Result<()> {
        info!(target: "dchat", "DCHAT::start()::start");

        let ex2 = ex.clone();

        self.register_protocol(self.msgs.clone()).await?;
        self.p2p.clone().start(ex.clone()).await?;
        ex2.spawn(self.p2p.clone().run(ex.clone())).detach();

        info!(target: "dchat", "DCHAT::start()::stop");
        Ok(())
    }

    async fn send(&self, message: String) -> Result<()> {
        let mut stdout = io::stdout().lock();
        stdout.write_all(b"sending: ")?;
        stdout.write_all(message.as_bytes())?;
        let dchatmsg = Dchatmsg { message };
        self.p2p.broadcast(dchatmsg).await?;
        Ok(())
    }
}

// inbound
fn alice() -> Result<Settings> {
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
    let log_level = get_log_level(1);
    let log_config = get_log_config();

    let log_path = "/tmp/dchat.log";
    let file = File::create(log_path).unwrap();
    WriteLogger::init(log_level, log_config, file)?;

    // TODO:: proper error handling
    let settings: Result<Settings> = match std::env::args().nth(1) {
        Some(id) => match id.as_str() {
            "a" => {
                println!("alice selected");
                alice()
            }
            "b" => {
                println!("bob selected");
                bob()
            }
            _ => {
                println!("you must specify either a or b");
                Err(Error::ConfigInvalid)
            }
        },
        None => {
            println!("you must specify either a or b");
            Err(Error::ConfigInvalid)
        }
    };

    let p2p = net::P2p::new(settings?.into()).await;

    //let p2p = p2p.clone();

    let nthreads = num_cpus::get();
    let (signal, shutdown) = async_channel::unbounded::<()>();

    let ex = Arc::new(Executor::new());
    let ex2 = ex.clone();

    let msgs: DchatmsgsBuffer = Arc::new(Mutex::new(vec![Dchatmsg { message: String::new() }]));

    let dchat = Dchat::new(p2p, msgs);

    let (_, result) = Parallel::new()
        .each(0..nthreads, |_| smol::future::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::future::block_on(async move {
                dchat.start(ex2).await?;
                dchat.render().await?;
                drop(signal);
                Ok(())
            })
        });

    result
}
