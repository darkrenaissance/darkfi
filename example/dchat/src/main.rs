use async_channel::{Receiver, Sender};
use async_executor::Executor;
use async_std::{net::TcpListener, sync::Arc};

use easy_parallel::Parallel;
//use futures::{AsyncRead, AsyncWrite};
use log::{error, info};
use simplelog::WriteLogger;
//use std::net::SocketAddr;
use std::{
    fs::File,
    io::{self, Write},
};

use structopt_toml::StructOptToml;
use termion::{event::Key, input::TermRead};

use darkfi::{
    net,
    system::{Subscriber, SubscriberPtr},
    util::{
        cli::{get_log_config, get_log_level, spawn_config},
        path::get_config_path,
    },
    Result,
};

use crate::{
    dchatmsg::Dchatmsg,
    protocol_dchat::ProtocolDchat,
    settings::{Args, CONFIG_FILE, CONFIG_FILE_CONTENTS},
};

pub mod dchatmsg;
pub mod protocol_dchat;
pub mod settings;

struct Dchat {
    //accept_addr: String,
    //connect_addr: String,
    //dchatmsgs_buffer: DchatmsgsBuffer,
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
                            Key::Char('s') => {}
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

    async fn register_protocol(
        &self,
        p2p_send_channel: Sender<Dchatmsg>,
        //p2p: net::P2pPtr,
    ) -> Result<()> {
        info!("DCHAT::register_protocol()::start");
        let registry = self.p2p.protocol_registry();
        registry
            .register(net::SESSION_ALL, move |channel, p2p| {
                let sender = p2p_send_channel.clone();
                async move { ProtocolDchat::init(channel, p2p).await }
            })
            .await;
        info!("DCHAT::register_protocol()::stop");
        Ok(())
    }

    async fn start(
        &self,
        executor: Arc<Executor<'_>>,
        settings: Args,
        p2p_recv_channel: Receiver<Dchatmsg>,
        p2p_send_channel: Sender<Dchatmsg>,
    ) -> Result<()> {
        info!("DCHAT::start()::start");
        let dchat = Dchat::new(self.p2p.clone());

        self.p2p.clone().start(executor.clone()).await?;

        dchat.register_protocol(p2p_send_channel, self.p2p.clone()).await?;

        let result = dchat.render(executor.clone()).await;

        if let Err(e) = result {
            error!("Rendering failed {}", e);
        };

        let executor_cloned = executor.clone();
        executor_cloned.spawn(self.p2p.clone().run(executor.clone())).detach();

        self.send().await?;

        let listenaddr = settings.listen.socket_addrs(|| None)?[0];
        let listener = TcpListener::bind(listenaddr).await?;

        executor
            .spawn(async move {
                loop {
                    let (_stream, peer_addr) = match listener.accept().await {
                        Ok((s, a)) => (s, a),
                        Err(e) => {
                            error!("failed accepting new connections: {}", e);
                            continue
                        }
                    };

                    info!("dchat accepted new client: {}", peer_addr);
                }
            })
            .detach();

        info!("DCHAT::start()::stop");
        Ok(())
    }

    async fn send(&self) -> Result<()> {
        let dchatmsg = Dchatmsg { message: "helloworld".to_string() };
        self.p2p.broadcast(dchatmsg).await?;
        Ok(())
    }
}

#[async_std::main]
async fn main() -> Result<()> {
    //let options = ProgramOptions::load()?;
    //let verbosity_level = options.app.occurrences_of("verbose");

    let args = Args::from_args_with_toml("").unwrap();
    let cfg_path = get_config_path(args.config, CONFIG_FILE)?;
    spawn_config(&cfg_path, CONFIG_FILE_CONTENTS.as_bytes())?;

    let log_level = get_log_level(args.verbose);
    let log_config = get_log_config();

    // TODO: clean up
    if args.log_path.is_none() {
        let log_path = "/tmp/dchat.log";
        let file = File::create(log_path).unwrap();
        WriteLogger::init(log_level, log_config, file)?;
    } else {
        let file = File::create(args.log_path.unwrap()).unwrap();
        WriteLogger::init(log_level, log_config, file)?;
    }

    let args = Args::from_args_with_toml(&std::fs::read_to_string(cfg_path)?).unwrap();

    let settings = args;
    let net_settings = settings.net.clone();

    let (p2p_send_channel, p2p_recv_channel) = async_channel::unbounded::<Dchatmsg>();
    let p2p = net::P2p::new(net_settings.into()).await;
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
                dchat.start(ex2, settings.clone(), p2p_recv_channel, p2p_send_channel).await?;
                drop(signal);
                Ok(())
            })
        });

    result
}
