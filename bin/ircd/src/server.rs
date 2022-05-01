use async_std::net::TcpStream;

use futures::{io::WriteHalf, AsyncWriteExt};
use log::{debug, info, warn};
use rand::{rngs::OsRng, RngCore};

use darkfi::{Error, Result};

use crate::{privmsg::Privmsg, SeenMsgIds};

pub struct IrcServerConnection {
    write_stream: WriteHalf<TcpStream>,
    is_nick_init: bool,
    is_user_init: bool,
    is_registered: bool,
    nickname: String,
    _channels: Vec<String>,
    seen_msg_id: SeenMsgIds,
    p2p_sender: async_channel::Sender<Privmsg>,
}

impl IrcServerConnection {
    pub fn new(
        write_stream: WriteHalf<TcpStream>,
        seen_msg_id: SeenMsgIds,
        p2p_sender: async_channel::Sender<Privmsg>,
    ) -> Self {
        Self {
            write_stream,
            is_nick_init: false,
            is_user_init: false,
            is_registered: false,
            nickname: "".to_string(),
            _channels: vec![],
            seen_msg_id,
            p2p_sender,
        }
    }

    pub async fn update(&mut self, line: String) -> Result<()> {
        let mut tokens = line.split_ascii_whitespace();
        // Commands can begin with :garbage but we will reject clients doing
        // that for now to keep the protocol simple and focused.
        let command = tokens.next().ok_or(Error::MalformedPacket)?;

        info!("Received command: {}", command);

        match command {
            "USER" => {
                // We can stuff any extra things like public keys in here.
                // Ignore it for now.
                self.is_user_init = true;
            }
            "NICK" => {
                let nickname = tokens.next().ok_or(Error::MalformedPacket)?;
                self.is_nick_init = true;
                let old_nick = std::mem::replace(&mut self.nickname, nickname.to_string());

                let nick_reply = format!(":{}!anon@dark.fi NICK {}\r\n", old_nick, self.nickname);
                self.reply(&nick_reply).await?;
            }
            "JOIN" => {
                // Ignore since channels are all autojoin
                // let channel = tokens.next().ok_or(Error::MalformedPacket)?;
                // self.channels.push(channel.to_string());

                // let join_reply = format!(":{}!anon@dark.fi JOIN {}\r\n", self.nickname, channel);
                // self.reply(&join_reply).await?;

                // self.write_stream.write_all(b":f00!f00@127.0.01 PRIVMSG #dev :y0\r\n").await?;
            }
            "PING" => {
                let line_clone = line.clone();
                let split_line: Vec<&str> = line_clone.split_whitespace().collect();
                if split_line.len() > 1 && split_line[0] == "PING" {
                    let pong = format!("PONG {}\r\n", split_line[1]);
                    self.reply(&pong).await?;
                }
            }
            "PRIVMSG" => {
                let channel = tokens.next().ok_or(Error::MalformedPacket)?;
                let substr_idx = line.find(':').ok_or(Error::MalformedPacket)?;

                if substr_idx >= line.len() {
                    return Err(Error::MalformedPacket)
                }

                let message = &line[substr_idx + 1..];
                info!("Message {}: {}", channel, message);

                let random_id = OsRng.next_u32();

                let protocol_msg = Privmsg {
                    id: random_id,
                    nickname: self.nickname.clone(),
                    channel: channel.to_string(),
                    message: message.to_string(),
                };

                let mut smi = self.seen_msg_id.lock().await;
                smi.push(random_id);
                drop(smi);

                self.p2p_sender.send(protocol_msg).await?;
            }
            "QUIT" => {
                // Close the connection
                return Err(Error::ServiceStopped)
            }
            _ => {
                warn!("Unimplemented `{}` command", command);
            }
        }

        if !self.is_registered && self.is_nick_init && self.is_user_init {
            debug!("Initializing peer connection");
            let register_reply = format!(":darkfi 001 {} :Let there be dark\r\n", self.nickname);
            self.reply(&register_reply).await?;
            self.is_registered = true;

            // Auto-joins
            macro_rules! autojoin {
                ($channel:expr,$topic:expr) => {
                    let j = format!(":{}!anon@dark.fi JOIN {}\r\n", self.nickname, $channel);
                    let t = format!(":DarkFi TOPIC {} :{}\r\n", $channel, $topic);
                    self.reply(&j).await?;
                    self.reply(&t).await?;
                };
            }

            autojoin!("#dev", "Development of DarkFi");
            autojoin!("#markets", "Markets, trading, DeFi, algo, biz, finance, and economics");
            autojoin!("#memes", "Memetic engineering");
        }

        Ok(())
    }

    pub async fn reply(&mut self, message: &str) -> Result<()> {
        self.write_stream.write_all(message.as_bytes()).await?;
        debug!("Sent {}", message);
        Ok(())
    }
}
