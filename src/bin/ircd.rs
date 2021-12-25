#[macro_use]
extern crate clap;
use std::{
    io,
    net::{SocketAddr, TcpListener, TcpStream},
    sync::Arc,
};

use async_executor::Executor;
use async_std::io::BufReader;
use futures::{
    io::{ReadHalf, WriteHalf},
    AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, Future, FutureExt,
};
use log::{debug, error, info, warn};
use simplelog::{ColorChoice, LevelFilter, TermLogger, TerminalMode};
use smol::Async;

use drk::{
    net,
    serial::{Decodable, Encodable},
    Error, Result,
};

/*
NICK fifififif
USER username 0 * :Real
:behemoth 001 fifififif :Hi, welcome to IRC
:behemoth 002 fifififif :Your host is behemoth, running version miniircd-2.1
:behemoth 003 fifififif :This server was created sometime
:behemoth 004 fifififif behemoth miniircd-2.1 o o
:behemoth 251 fifififif :There are 1 users and 0 services on 1 server
:behemoth 422 fifififif :MOTD File is missing
JOIN #dev
:fifififif!username@127.0.0.1 JOIN #dev
:behemoth 331 fifififif #dev :No topic is set
:behemoth 353 fifififif = #dev :fifififif
:behemoth 366 fifififif #dev :End of NAMES list
PRIVMSG #dev hihi
*/

struct ServerConnection {
    write_stream: WriteHalf<Async<TcpStream>>,
    is_nick_init: bool,
    is_user_init: bool,
    is_registered: bool,
    nickname: String,
    channels: Vec<String>,
}

impl ServerConnection {
    fn new(write_stream: WriteHalf<Async<TcpStream>>) -> Self {
        ServerConnection {
            write_stream,
            is_nick_init: false,
            is_user_init: false,
            is_registered: false,
            nickname: "".to_string(),
            channels: vec![],
        }
    }

    async fn update(&mut self, line: String, p2p: net::P2pPtr) -> Result<()> {
        let mut tokens = line.split_ascii_whitespace();
        // Commands can begin with :garbage but we will reject clients doing that for now
        // to keep the protocol simple and focused.
        let command = tokens.next().ok_or(Error::MalformedPacket)?;

        debug!("Received command: {}", command);

        match command {
            "NICK" => {
                let nickname = tokens.next().ok_or(Error::MalformedPacket)?;
                self.is_nick_init = true;
                self.nickname = nickname.to_string();
            }
            "USER" => {
                // We can stuff any extra things like public keys in here
                // Ignore it for now
                self.is_user_init = true;
            }
            "JOIN" => {
                // Ignore since channels are all autojoin
                //let channel = tokens.next().ok_or(Error::MalformedPacket)?;
                //self.channels.push(channel.to_string());

                //let join_reply = format!(":{}!darkfi@127.0.0.1 JOIN {}\n", self.nickname,
                // channel); self.reply(&join_reply).await?;

                //self.write_stream.write_all(b":f00!f00@127.0.0.1 PRIVMSG #dev :y0\n").await?;
            }
            "PING" => {
                self.reply("PONG").await?;
            }
            "PRIVMSG" => {
                let channel = tokens.next().ok_or(Error::MalformedPacket)?;

                let substr_idx = line.find(':').ok_or(Error::MalformedPacket)?;
                if substr_idx >= line.len() {
                    return Err(Error::MalformedPacket)
                }
                let message = &line[substr_idx + 1..];
                info!("Message {}: {}", channel, message);

                let protocol_msg = PrivMsg {
                    nickname: self.nickname.clone(),
                    channel: channel.to_string(),
                    message: message.to_string(),
                };
                p2p.broadcast(protocol_msg).await?;
            }
            _ => {}
        }

        if !self.is_registered && self.is_nick_init && self.is_user_init {
            debug!("Initializing peer connection");
            let register_reply = format!(":darkfi 001 {} :Let there be dark\n", self.nickname);
            self.reply(&register_reply).await?;
            self.is_registered = true;

            // Auto-joins
            for channel in ["#dev", "#markets", "#welcome"] {
                let join_reply = format!(":{}!darkfi@127.0.0.1 JOIN {}\n", self.nickname, channel);
                self.reply(&join_reply).await?;
            }
        }

        Ok(())
    }

    async fn reply(&mut self, message: &str) -> Result<()> {
        self.write_stream.write_all(message.as_bytes()).await?;
        debug!("Sent {}", message);
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct PrivMsg {
    nickname: String,
    channel: String,
    message: String,
}

impl net::Message for PrivMsg {
    fn name() -> &'static str {
        "privmsg"
    }
}

impl Encodable for PrivMsg {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.nickname.encode(&mut s)?;
        len += self.channel.encode(&mut s)?;
        len += self.message.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for PrivMsg {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            nickname: Decodable::decode(&mut d)?,
            channel: Decodable::decode(&mut d)?,
            message: Decodable::decode(&mut d)?,
        })
    }
}

async fn process(
    recvr: async_channel::Receiver<Arc<PrivMsg>>,
    stream: Async<TcpStream>,
    peer_addr: SocketAddr,
    p2p: net::P2pPtr,
    executor: Arc<Executor<'_>>,
) -> Result<()> {
    //stream.write_all(b":behemoth 001 fifififif :Hi, welcome to IRC").await;
    //stream.write_all(b"NICK username");
    //stream.write_all(b"USER username 0 * :username");
    //stream.write_all(b"JOIN #dev");
    //stream.write_all(b"PRIVMSG #dev y0");

    // PING :behemoth

    let (reader, writer) = stream.split();

    let mut reader = BufReader::new(reader);
    let mut connection = ServerConnection::new(writer);

    loop {
        let mut line = String::new();
        futures::select! {
            privmsg = recvr.recv().fuse() => {
                let privmsg = privmsg.expect("internal message queue error");
                debug!("ABOUT TO SEND {:?}", privmsg);
                let irc_msg = format!(
                    ":{}!darkfi@127.0.0.1 PRIVMSG {} :{}\n",
                    privmsg.nickname,
                    privmsg.channel,
                    privmsg.message
                );

                connection.reply(&irc_msg).await?;
            }
            err = reader.read_line(&mut line).fuse() => {
                if let Err(err) = err {
                    warn!("Read line error. Closing stream for {}: {}", peer_addr, err);
                    return Ok(())
                }
                process_user_input(line, peer_addr, &mut connection, p2p.clone()).await;
            }
        };
    }
}

async fn process_user_input(
    mut line: String,
    peer_addr: SocketAddr,
    connection: &mut ServerConnection,
    p2p: net::P2pPtr,
) {
    if line.len() == 0 {
        warn!("Received empty line from {}. Closing connection.", peer_addr);
        return
    }
    assert!(&line[(line.len() - 1)..] == "\n");
    // Remove the \n character
    line.pop();

    debug!("Received '{}' from {}", line, peer_addr);

    if let Err(err) = connection.update(line, p2p.clone()).await {
        warn!("Connection error: {} for {}", err, peer_addr);
        return
    }
}

struct ProtocolPrivMsg {
    notify_queue_sender: async_channel::Sender<Arc<PrivMsg>>,
    privmsg_sub: net::MessageSubscription<PrivMsg>,
    jobsman: net::ProtocolJobsManagerPtr,
}

impl ProtocolPrivMsg {
    async fn new(
        channel: net::ChannelPtr,
        notify_queue_sender: async_channel::Sender<Arc<PrivMsg>>,
    ) -> Arc<Self> {
        let message_subsytem = channel.get_message_subsystem();
        message_subsytem.add_dispatch::<PrivMsg>().await;

        debug!("ADDED DISPATCH");

        let privmsg_sub =
            channel.subscribe_msg::<PrivMsg>().await.expect("Missing PrivMsg dispatcher!");

        Arc::new(Self {
            notify_queue_sender,
            privmsg_sub,
            jobsman: net::ProtocolJobsManager::new("PrivMsgProtocol", channel),
        })
    }

    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) {
        debug!(target: "ircd", "ProtocolPrivMsg::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_privmsg(), executor.clone()).await;
        debug!(target: "ircd", "ProtocolPrivMsg::start() [END]");
    }

    async fn handle_receive_privmsg(self: Arc<Self>) -> Result<()> {
        debug!(target: "ircd", "ProtocolAddress::handle_receive_privmsg() [START]");
        loop {
            let privmsg = self.privmsg_sub.receive().await?;

            debug!(
                target: "ircd",
                "ProtocolPrivMsg::handle_receive_privmsg() received {:?}",
                privmsg
            );

            self.notify_queue_sender.send(privmsg).await.expect("notify_queue_sender send failed!");
        }
    }
}

async fn channel_loop(
    p2p: net::P2pPtr,
    sender: async_channel::Sender<Arc<PrivMsg>>,
    executor: Arc<Executor<'_>>,
) -> Result<()> {
    debug!("CHANNEL SUBS LOOP");
    let new_channel_sub = p2p.subscribe_channel().await;

    loop {
        let channel = new_channel_sub.receive().await?;

        debug!("NEWCHANNEL");

        let protocol_privmsg = ProtocolPrivMsg::new(channel, sender.clone()).await;
        protocol_privmsg.start(executor.clone()).await;
    }
}

async fn start(executor: Arc<Executor<'_>>, options: ProgramOptions) -> Result<()> {
    let listener = match Async::<TcpListener>::bind(options.irc_accept_addr) {
        Ok(listener) => listener,
        Err(err) => {
            error!("Bind listener failed: {}", err);
            return Err(Error::OperationFailed)
        }
    };
    let local_addr = match listener.get_ref().local_addr() {
        Ok(addr) => addr,
        Err(err) => {
            error!("Failed to get local address: {}", err);
            return Err(Error::OperationFailed)
        }
    };
    info!("Listening on {}", local_addr);

    let p2p = net::P2p::new(options.network_settings);
    // Performs seed session
    p2p.clone().start(executor.clone()).await?;
    // Actual main p2p session
    let ex2 = executor.clone();
    let p2p2 = p2p.clone();
    executor
        .spawn(async move {
            if let Err(err) = p2p2.run(ex2).await {
                error!("Error: p2p run failed {}", err);
            }
        })
        .detach();

    let (sender, recvr) = async_channel::unbounded();

    // todo: be careful of zombie processes
    // for now we just want things to work
    executor.spawn(channel_loop(p2p.clone(), sender, executor.clone())).detach();

    loop {
        let (stream, peer_addr) = match listener.accept().await {
            Ok((s, a)) => (s, a),
            Err(err) => {
                error!("Error listening for connections: {}", err);
                return Err(Error::ServiceStopped)
            }
        };
        info!("Accepted client: {}", peer_addr);

        let p2p2 = p2p.clone();
        let ex2 = executor.clone();
        executor.spawn(process(recvr.clone(), stream, peer_addr, p2p2, ex2)).detach();
    }
}

struct ProgramOptions {
    network_settings: net::Settings,
    log_path: Box<std::path::PathBuf>,
    irc_accept_addr: SocketAddr,
}

impl ProgramOptions {
    fn load() -> Result<ProgramOptions> {
        let app = clap_app!(dfi =>
            (version: "0.1.0")
            (author: "Amir Taaki <amir@dyne.org>")
            (about: "Dark node")
            (@arg ACCEPT: -a --accept +takes_value "Accept address")
            (@arg SEED_NODES: -s --seeds +takes_value ... "Seed nodes")
            (@arg CONNECTS: -c --connect +takes_value ... "Manual connections")
            (@arg CONNECT_SLOTS: --slots +takes_value "Connection slots")
            (@arg LOG_PATH: --log +takes_value "Logfile path")
            (@arg IRC_ACCEPT: -r --irc +takes_value "IRC accept address")
        )
        .get_matches();

        let accept_addr = if let Some(accept_addr) = app.value_of("ACCEPT") {
            Some(accept_addr.parse()?)
        } else {
            None
        };

        let mut seed_addrs: Vec<SocketAddr> = vec![];
        if let Some(seeds) = app.values_of("SEED_NODES") {
            for seed in seeds {
                seed_addrs.push(seed.parse()?);
            }
        }

        let mut manual_connects: Vec<SocketAddr> = vec![];
        if let Some(connections) = app.values_of("CONNECTS") {
            for connect in connections {
                manual_connects.push(connect.parse()?);
            }
        }

        let connection_slots = if let Some(connection_slots) = app.value_of("CONNECT_SLOTS") {
            connection_slots.parse()?
        } else {
            0
        };

        let log_path = Box::new(
            if let Some(log_path) = app.value_of("LOG_PATH") {
                std::path::Path::new(log_path)
            } else {
                std::path::Path::new("/tmp/darkfid.log")
            }
            .to_path_buf(),
        );

        let irc_accept_addr = if let Some(accept_addr) = app.value_of("IRC_ACCEPT") {
            accept_addr.parse()?
        } else {
            ([127, 0, 0, 1], 6667).into()
        };

        Ok(ProgramOptions {
            network_settings: net::Settings {
                inbound: accept_addr,
                outbound_connections: connection_slots,
                external_addr: accept_addr,
                peers: manual_connects,
                seeds: seed_addrs,
                ..Default::default()
            },
            log_path,
            irc_accept_addr,
        })
    }
}

fn main() -> Result<()> {
    TermLogger::init(
        LevelFilter::Debug,
        simplelog::Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )?;

    let options = ProgramOptions::load()?;

    let ex = Arc::new(Executor::new());
    smol::block_on(ex.run(start(ex.clone(), options)))
}
