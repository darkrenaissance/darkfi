use async_executor::Executor;
use async_std::io::BufReader;
use futures::{
    io::{ReadHalf, WriteHalf},
    AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, Future, FutureExt,
};
use log::{debug, error, info, warn};
use smol::Async;
use std::{
    io,
    net::{SocketAddr, TcpListener, TcpStream},
    sync::Arc,
};

use drk::{Error, Result};

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
                let channel = tokens.next().ok_or(Error::MalformedPacket)?;
                self.channels.push(channel.to_string());

                let join_reply = format!(":{}!darkfi@127.0.0.1 JOIN {}\n", self.nickname, channel);
                self.reply(&join_reply).await?;

                //self.write_stream.write_all(b":f00!f00@127.0.0.1 PRIVMSG #dev :y0\n").await?;
            }
            "PING" => {
                self.reply("PONG").await?;
            }
            _ => {}
        }

        if !self.is_registered && self.is_nick_init && self.is_user_init {
            debug!("Initializing peer connection");
            let register_reply = format!(":darkfi 001 {} :Let there be dark\n", self.nickname);
            self.reply(&register_reply).await?;
            self.is_registered = true;
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

async fn async_main(executor: Arc<Executor<'_>>) -> Result<()> {
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

fn main() -> Result<()> {
    //simple_logger::init_with_level(log::Level::Trace)?;
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Debug)
        .with_utc_timestamps()
        .init()?;

    let ex = Arc::new(Executor::new());
    smol::block_on(ex.run(async_main(ex.clone())))
}
