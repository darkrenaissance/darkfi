use async_channel::Receiver;
use async_executor::Executor;
use async_std::{
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
};
use clap::{Parser, Subcommand};
use futures::{io::WriteHalf, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use smol::Async;
use url::Url;

use darkfi::{
    async_daemonize, cli_desc, net,
    system::{Subscriber, SubscriberPtr},
    util::{
        cli::{get_log_config, get_log_level, spawn_config},
        path::get_config_path,
    },
    Result,
};
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use smol::future;
use structopt_toml::StructOptToml;

use crate::{
    dchatmsg::{Dchatmsg, DchatmsgsBuffer},
    protocol_dchat::ProtocolDchat,
    settings::{CONFIG_FILE, CONFIG_FILE_CONTENTS},
};

pub mod dchatmsg;
pub mod protocol_dchat;
pub mod server;
pub mod settings;

const SIZE_OF_MSGS_BUFFER: usize = 4096;

#[derive(Parser)]
#[clap(name = "dchat", about = cli_desc!(), version)]
#[clap(arg_required_else_help(true))]
struct Args {
    #[clap(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,

    #[clap(subcommand)]
    command: Option<Dchatsubcommand>,
}

#[derive(Subcommand)]
enum Dchatsubcommand {
    Inbox,
    Send { msg: String, addr: String },
    Receive { addr: String },
}

#[async_std::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let log_level = get_log_level(args.verbose.into());
    let log_config = get_log_config();
    TermLogger::init(log_level, log_config, TerminalMode::Mixed, ColorChoice::Auto)?;

    let dchat = Dchat::new();

    match args.command {
        Some(sc) => match sc {
            Dchatsubcommand::Inbox => {
                eprintln!("inbox");
            }
            Dchatsubcommand::Send { msg, addr } => {
                dchat.send(msg, addr).await?;
            }
            Dchatsubcommand::Receive { addr } => {
                dchat.receive(addr).await?;
            }
        },
        None => {}
    }
    Ok(())
}
struct Dchat {
    //dchatmsgs_buffer: DchatmsgsBuffer,
    //p2p: net::P2pPtr,
    //senders: SubscriberPtr<Dchatmsg>,
}

impl Dchat {
    //fn new(dchatmsgs_buffer: DchatmsgsBuffer, p2p: net::P2pPtr) -> Self {
    //    let senders = Subscriber::new();
    //    Self { dchatmsgs_buffer, p2p, senders }
    //}
    fn new() -> Arc<Self> {
        Arc::new(Self {})
    }

    async fn receive(self: Arc<Self>, addr: String) -> Result<()> {
        smol::block_on(async {
            let listener = TcpListener::bind(&addr).await?;
            eprintln!("Listening on {}", listener.local_addr()?);
            loop {
                let (stream, peer_addr) = listener.accept().await?;
                println!("Accepted client: {}", peer_addr);
                smol::spawn(self.clone().read_msg(stream)).detach();
            }
        })
    }

    async fn send(&self, msg: String, addr: String) -> Result<()> {
        let mut stream = TcpStream::connect(&addr).await?;
        eprintln!("Connected to {}", stream.local_addr()?);
        stream.write_all(msg.as_bytes()).await?;
        eprintln!("Sending '{}'", msg);
        Ok(())
    }

    async fn read_msg(self: Arc<Self>, mut stream: TcpStream) -> Result<()> {
        let mut buffer = [0u8; 4];
        stream.read_exact(&mut buffer).await?;
        let buffer = std::str::from_utf8(&buffer).unwrap();
        eprintln!("{}", buffer);
        Ok(())
    }
    //fn start_p2p_receive_loop(
    //    &self,
    //    executor: Arc<Executor<'_>>,
    //    p2p_receiver: Receiver<Dchatmsg>,
    //) {
    //    let senders = self.senders.clone();
    //    executor
    //        .spawn(async move {
    //            while let Ok(msg) = p2p_receiver.recv().await {
    //                senders.notify(msg).await;
    //            }
    //        })
    //        .detach();
    //}
}

//#[async_std::main]
//async_daemonize!(realmain);
//async fn realmain(settings: Args, executor: Arc<Executor<'_>>) -> Result<()> {
//    //let dchatmsgs_buffer: DchatmsgsBuffer =
//    //    Arc::new(Mutex::new(ringbuffer::AllocRingBuffer::with_capacity(SIZE_OF_MSGS_BUFFER)));
//    //// Pick up channel settings from the TOML configuration
//    //let cfg_path = get_config_path(settings.config, CONFIG_FILE)?;
//
//    //let net_settings = settings.net;
//    //let (p2p_send_channel, p2p_recv_channel) = async_channel::unbounded::<Dchatmsg>();
//    //let p2p = net::P2p::new(net_settings.into()).await;
//    //let p2p = p2p.clone();
//
//    //let registry = p2p.protocol_registry();
//
//    //let dchatmsgs_buffer_cloned = dchatmsgs_buffer.clone();
//
//    //registry
//    //    .register(net::SESSION_ALL, move |channel, p2p| {
//    //        let sender = p2p_send_channel.clone();
//    //        let privmsgs_buffer_cloned = dchatmsgs_buffer_cloned.clone();
//    //        async move { ProtocolDchat::init(channel, sender, p2p, privmsgs_buffer_cloned).await }
//    //    })
//    //    .await;
//
//    //p2p.clone().start(executor.clone()).await?;
//
//    //let executor_cloned = executor.clone();
//    //executor_cloned.spawn(p2p.clone().run(executor.clone())).detach();
//
//    //let listenaddr = settings.listen.socket_addrs(|| None)?[0];
//    //let listener = TcpListener::bind(listenaddr).await?;
//
//    //let executor_cloned = executor.clone();
//    //executor
//    //    .spawn(async move {
//    //        let dchat = Dchatd::new(dchatmsgs_buffer.clone(), p2p.clone());
//
//    //        dchat.start_p2p_receive_loop(executor_cloned.clone(), p2p_recv_channel);
//
//    //        loop {
//    //            let (stream, peer_addr) = match listener.accept().await {
//    //                Ok((s, a)) => (s, a),
//    //                Err(e) => {
//    //                    //error!("failed accepting new connections: {}", e);
//    //                    continue;
//    //                }
//    //            };
//
//    //            //ircd.process_new_connection(executor_cloned.clone(), stream, peer_addr).await
//
//    //            //if let Err(e) = result {
//    //            //    error!("Failed processing connection {}: {}", peer_addr, e);
//    //            //    continue;
//    //            //};
//
//    //            //info!("IRC Accepted new client: {}", peer_addr);
//    //        }
//    //    })
//    //    .detach();
//
//    //let (signal, shutdown) = async_channel::bounded::<()>(1);
//
//    //
//    //shutdown.recv().await?;
//
//    Ok(())
//}
