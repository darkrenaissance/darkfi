use async_channel::{Receiver, Sender};
use async_executor::Executor;
use async_std::sync::Arc;
use easy_parallel::Parallel;
use log::{error, info};
use simplelog::WriteLogger;
use std::{
    fs::File,
    io::{self, Read, Write},
    path::PathBuf,
};
use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};
use termion::{async_stdin, event::Key, input::TermRead};
use toml::Value;
use url::Url;

use darkfi::{
    net,
    net::Settings,
    util::{
        cli::{get_log_config, get_log_level, spawn_config},
        path::join_config_path,
    },
    Result,
};

use crate::{dchatmsg::Dchatmsg, protocol_dchat::ProtocolDchat};
use darkfi::net::settings::SettingsOpt;

pub mod dchatmsg;
pub mod protocol_dchat;

const CONFIG_FILE_CONTENTS: &str = include_str!("../dchat_config.toml");

struct Dchat {
    p2p: net::P2pPtr,
    //msg_sub: net::MessageSubscription<Dchatmsg>,
}

impl Dchat {
    fn new(p2p: net::P2pPtr) -> Arc<Self> {
        Arc::new(Self { p2p })
    }

    async fn render(&self, ex: Arc<Executor<'_>>) -> Result<()> {
        info!(target: "dchat", "DCHAT::render()::start");
        let mut stdout = io::stdout().lock();
        let mut stdin = async_stdin();

        stdout.write_all(
            b"Welcome to dchat
    s: send message
    i. inbox
    q: quit \n",
        )?;

        loop {
            for k in stdin.by_ref().keys() {
                match k.unwrap() {
                    Key::Char('q') => {
                        info!(target: "dchat", "DCHAT::Q pressed.... exiting");
                        return Ok(())
                    }
                    Key::Char('i') => {}

                    Key::Char('s') => {
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

    async fn register_protocol(&self, p2p_send_channel: Sender<Dchatmsg>) -> Result<()> {
        info!(target: "dchat", "dchat::register_protocol()::start");
        let registry = self.p2p.protocol_registry();
        registry
            .register(net::SESSION_ALL, move |channel, p2p| {
                let sender = p2p_send_channel.clone();
                async move { ProtocolDchat::init(channel, p2p, sender).await }
            })
            .await;
        info!(target: "dchat", "DCHAT::register_protocol()::stop");
        Ok(())
    }

    async fn start(&self, ex: Arc<Executor<'_>>) -> Result<()> {
        info!(target: "dchat", "DCHAT::start()::start");

        //let sub = net::MessageSubscription::new();
        let (p2p_send_channel, p2p_recv_channel) = async_channel::unbounded::<Dchatmsg>();

        let ex2 = ex.clone();
        let ex3 = ex.clone();

        let dchat = Dchat::new(self.p2p.clone());

        dchat.register_protocol(p2p_send_channel).await?;

        self.p2p.clone().start(ex.clone()).await?;
        ex2.spawn(self.p2p.clone().run(ex.clone())).detach();
        self.start_p2p_receive_loop(ex3, p2p_recv_channel);

        info!(target: "dchat", "DCHAT::start()::stop");
        Ok(())
    }

    fn start_p2p_receive_loop(
        &self,
        executor: Arc<Executor<'_>>,
        p2p_receiver: Receiver<Dchatmsg>,
    ) {
        //let senders = self.senders.clone();
        executor
            .spawn(async move {
                while let Ok(msg) = p2p_receiver.recv().await {
                    info!(target: "dchat", "START P2P RECEIVE LOOP:: RECEIVED MSG {:?}", msg);
                    //senders.notify(msg).await;
                }
            })
            .detach();
    }
    //async fn send_loop(&self) -> Result<()> {
    //    let dchatmsg = Dchatmsg { message };
    //    loop {

    //    }
    //}
    async fn send(&self, message: String) -> Result<()> {
        let mut stdout = io::stdout().lock();
        stdout.write_all(b"sending: ")?;
        stdout.write_all(message.as_bytes())?;
        let dchatmsg = Dchatmsg { message };
        match self.p2p.broadcast(dchatmsg).await {
            Ok(_o) => {
                info!(target: "dchat", "SEND: MSG BROADCAST SUCCESSFULLY");
            }
            Err(e) => {
                error!(target: "dchat", "SEND: MSG FAILED TO BROADCAST {}", e);
            }
        }
        Ok(())
    }
}

fn get_settings_from_config(flag: String) -> Result<SettingsOpt> {
    let name = format!("dchat_{}.toml", flag);
    let cfg_path = join_config_path(&PathBuf::from(name))?;
    spawn_config(&cfg_path, CONFIG_FILE_CONTENTS.as_bytes())?;
    let config: SettingsOpt =
        SettingsOpt::from_args_with_toml(&std::fs::read_to_string(cfg_path)?).unwrap();
    Ok(config)
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

    // TODO: error
    let settings: SettingsOpt = match std::env::args()
        .nth(1)
        .expect("string identifier missing: please pass a string to name your dchat instance")
    {
        flag => get_settings_from_config(flag)?,
    };

    let p2p = net::P2p::new(settings.into()).await;

    let p2p = p2p.clone();

    let nthreads = num_cpus::get();
    let (signal, shutdown) = async_channel::unbounded::<()>();

    let ex = Arc::new(Executor::new());
    let ex2 = ex.clone();
    let ex3 = ex.clone();

    let dchat = Dchat::new(p2p.clone());

    let (_, result) = Parallel::new()
        .each(0..nthreads, |_| smol::future::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::future::block_on(async move {
                dchat.start(ex3).await?;
                dchat.render(ex2).await?;
                drop(signal);
                Ok(())
            })
        });

    result
}
