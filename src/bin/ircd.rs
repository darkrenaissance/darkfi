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

use drk::{Error, Result, net};

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

    async fn update(&mut self, line: String) -> Result<()> {
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

                //let join_reply = format!(":{}!darkfi@127.0.0.1 JOIN {}\n", self.nickname, channel);
                //self.reply(&join_reply).await?;

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
        debug!("Sending {}", message);
        self.write_stream.write_all(message.as_bytes()).await?;
        Ok(())
    }
}

async fn process(stream: Async<TcpStream>, peer_addr: SocketAddr) {
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
        if let Err(err) = reader.read_line(&mut line).await {
            warn!("Read line error. Closing stream for {}: {}", peer_addr, err);
            return
        }
        if line.len() == 0 {
            warn!("Received empty line from {}. Closing connection.", peer_addr);
            return
        }
        assert!(&line[(line.len() - 1)..] == "\n");
        // Remove the \n character
        line.pop();

        debug!("Received '{}' from {}", line, peer_addr);

        if let Err(err) = connection.update(line).await {
            warn!("Connection error: {} for {}", err, peer_addr);
            return
        }
    }
}

async fn start(executor: Arc<Executor<'_>>, options: ProgramOptions) -> Result<()> {
    let accept_addr = ([127, 0, 0, 1], 6667);
    let listener = match Async::<TcpListener>::bind(accept_addr) {
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

    /*
    let p2p = net::P2p::new(options.network_settings);
    // Performs seed session
    p2p.clone().start(executor.clone()).await?;
    // Actual main p2p session
    let ex2 = executor.clone();
    executor.spawn(async move {
        if let Err(err) = p2p.run(ex2).await {
            error!("Error: p2p run failed {}", err);
        }
    }).detach();
    */

    loop {
        let (stream, peer_addr) = match listener.accept().await {
            Ok((s, a)) => (s, a),
            Err(err) => {
                error!("Error listening for connections: {}", err);
                return Err(Error::ServiceStopped)
            }
        };
        info!("Accepted client: {}", peer_addr);

        executor.spawn(process(stream, peer_addr)).detach();
    }
}

struct ProgramOptions {
    network_settings: net::Settings,
    log_path: Box<std::path::PathBuf>,
}

impl ProgramOptions {
    fn load() -> Result<ProgramOptions> {
        let app = clap_app!(dfi =>
            (version: "0.1.0")
            (author: "Amir Taaki <amir@dyne.org>")
            (about: "Dark node")
            (@arg ACCEPT: -a --accept +takes_value "Accept address")
            (@arg SEED_NODES: -s --seeds ... "Seed nodes")
            (@arg CONNECTS: -c --connect ... "Manual connections")
            (@arg CONNECT_SLOTS: --slots +takes_value "Connection slots")
            (@arg LOG_PATH: --log +takes_value "Logfile path")
            (@arg RPC_PORT: -r --rpc +takes_value "RPC port")
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

