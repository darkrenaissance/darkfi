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

use crate::privmsg::PrivMsg;

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

pub struct IrcServerConnection {
    write_stream: WriteHalf<Async<TcpStream>>,
    is_nick_init: bool,
    is_user_init: bool,
    is_registered: bool,
    nickname: String,
    channels: Vec<String>,
}

impl IrcServerConnection {
    pub fn new(write_stream: WriteHalf<Async<TcpStream>>) -> Self {
        Self {
            write_stream,
            is_nick_init: false,
            is_user_init: false,
            is_registered: false,
            nickname: "".to_string(),
            channels: vec![],
        }
    }

    pub async fn update(&mut self, line: String, p2p: net::P2pPtr) -> Result<()> {
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

    pub async fn reply(&mut self, message: &str) -> Result<()> {
        self.write_stream.write_all(message.as_bytes()).await?;
        debug!("Sent {}", message);
        Ok(())
    }
}

