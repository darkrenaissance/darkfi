use async_executor::Executor;
use async_std::sync::{Arc, Mutex};
use easy_parallel::Parallel;

use std::{
    fs::File,
    io::{stdin, stdout, Read, Write},
};

use log::debug;
use simplelog::WriteLogger;
use url::Url;

use termion::{event::Key, input::TermRead, raw::IntoRawMode};

use darkfi::{
    net,
    net::Settings,
    util::cli::{get_log_config, get_log_level},
};

use crate::{
    dchatmsg::{Dchatmsg, DchatmsgsBuffer},
    error::{Error, MissingSpecifier, Result},
    protocol_dchat::ProtocolDchat,
};

pub mod dchatmsg;
pub mod error;
pub mod protocol_dchat;

struct Dchat {
    p2p: net::P2pPtr,
    recv_msgs: DchatmsgsBuffer,
    input: String,
    display: DisplayMode,
}

enum DisplayMode {
    Normal,
    Editing,
    Inbox,
    MessageSent,
    SendFailed(Error),
}

impl Dchat {
    fn new(
        p2p: net::P2pPtr,
        recv_msgs: DchatmsgsBuffer,
        input: String,
        display: DisplayMode,
    ) -> Self {
        Self { p2p, recv_msgs, input, display }
    }

    async fn menu(&mut self) -> Result<()> {
        debug!(target: "dchat", "Dchat::menu() [START]");
        let stdout = stdout();
        let mut stdout = stdout.lock().into_raw_mode().unwrap();
        let mut stdin = stdin();

        loop {
            self.render().await?;
            for k in stdin.by_ref().keys() {
                match &self.display {
                    DisplayMode::Normal => match k.unwrap() {
                        Key::Char('q') => return Ok(()),
                        Key::Char('i') => {
                            self.display = DisplayMode::Inbox;
                            break
                        }

                        Key::Char('s') => {
                            self.display = DisplayMode::Editing;
                            break
                        }
                        _ => {}
                    },
                    DisplayMode::Editing => match k.unwrap() {
                        Key::Char('q') => return Ok(()),
                        Key::Char('\n') => {
                            match self.send().await {
                                Ok(_) => {
                                    self.display = DisplayMode::MessageSent;
                                }
                                Err(e) => {
                                    self.display = DisplayMode::SendFailed(e);
                                }
                            }
                            break
                        }
                        Key::Char(c) => {
                            self.input.push(c);
                        }
                        Key::Esc => {
                            self.display = DisplayMode::Normal;
                            break
                        }
                        _ => {}
                    },
                    DisplayMode::MessageSent => match k.unwrap() {
                        Key::Char('q') => return Ok(()),
                        Key::Esc => {
                            self.display = DisplayMode::Normal;
                            break
                        }
                        _ => {}
                    },
                    DisplayMode::Inbox => match k.unwrap() {
                        Key::Char('q') => return Ok(()),
                        _ => {}
                    },
                    DisplayMode::SendFailed(_) => match k.unwrap() {
                        Key::Char('q') => return Ok(()),
                        Key::Esc => {
                            self.display = DisplayMode::Normal;
                            break
                        }
                        _ => {}
                    },
                }
            }
            stdout.flush()?;
        }
    }

    async fn render(&mut self) -> Result<()> {
        debug!(target: "dchat", "Dchat::render() [START]");
        let stdout = stdout();
        let mut stdout = stdout.lock().into_raw_mode().unwrap();

        match &self.display {
            DisplayMode::Normal => {
                write!(
                    stdout,
                    "{}{}{}Welcome to dchat. {} s: send message {} i: inbox {} q: quit {}",
                    termion::clear::All,
                    termion::style::Bold,
                    termion::cursor::Goto(1, 2),
                    termion::cursor::Goto(1, 3),
                    termion::cursor::Goto(1, 4),
                    termion::cursor::Goto(1, 5),
                    termion::cursor::Goto(1, 6)
                )?;
                stdout.flush()?;
            }
            DisplayMode::Editing => {
                write!(
                    stdout,
                    "{}{}{}enter your msg.{} esc: stop editing {} enter: send {}",
                    termion::clear::All,
                    termion::style::Bold,
                    termion::cursor::Goto(1, 2),
                    termion::cursor::Goto(1, 3),
                    termion::cursor::Goto(1, 4),
                    termion::cursor::Goto(1, 5)
                )?;
                stdout.flush()?;
            }
            DisplayMode::Inbox => {
                let msgs = self.recv_msgs.lock().await;
                for i in msgs.iter() {
                    if !i.message.is_empty() {
                        write!(
                            stdout,
                            "{}{}{}received msg: {}",
                            termion::clear::All,
                            termion::style::Bold,
                            termion::cursor::Goto(1, 2),
                            i.message
                        )?;
                    } else {
                        write!(
                            stdout,
                            "{}{}{}inbox is empty",
                            termion::clear::All,
                            termion::style::Bold,
                            termion::cursor::Goto(1, 2),
                        )?;
                    }
                }
                stdout.flush()?;
            }
            DisplayMode::MessageSent => {
                write!(
                    stdout,
                    "{}{}{}message sent! {} esc: return to main menu {}",
                    termion::clear::All,
                    termion::style::Bold,
                    termion::cursor::Goto(1, 2),
                    termion::cursor::Goto(1, 3),
                    termion::cursor::Goto(1, 4),
                )?;
                stdout.flush()?;
            }
            DisplayMode::SendFailed(e) => {
                write!(
                    stdout,
                    "{}{}{}send message failed! reason: {} {} esc: return to main menu {}",
                    termion::clear::All,
                    termion::style::Bold,
                    termion::cursor::Goto(1, 2),
                    e,
                    termion::cursor::Goto(1, 3),
                    termion::cursor::Goto(1, 4),
                )?;
                stdout.flush()?;
            }
        }

        Ok(())
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

    async fn start(&self, ex: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "dchat", "Dchat::start() [START]");

        let ex2 = ex.clone();

        self.register_protocol(self.recv_msgs.clone()).await?;
        self.p2p.clone().start(ex.clone()).await?;
        ex2.spawn(self.p2p.clone().run(ex.clone())).detach();

        debug!(target: "dchat", "Dchat::start() [STOP]");
        Ok(())
    }

    async fn send(&self) -> Result<()> {
        let message = self.input.clone();
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

    let msgs: DchatmsgsBuffer = Arc::new(Mutex::new(vec![Dchatmsg { message: String::new() }]));

    let mut dchat = Dchat::new(p2p, msgs, String::new(), DisplayMode::Normal);

    let (_, result) = Parallel::new()
        .each(0..nthreads, |_| smol::future::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::future::block_on(async move {
                dchat.start(ex2).await?;
                dchat.menu().await?;
                drop(signal);
                Ok(())
            })
        });

    result
}
