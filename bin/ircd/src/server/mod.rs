use std::net::SocketAddr;

use futures::{io::WriteHalf, AsyncRead, AsyncWrite, AsyncWriteExt};
use fxhash::FxHashMap;
use log::{debug, info, warn};

use darkfi::{net::P2pPtr, system::SubscriberPtr, Error, Result};

use crate::{
    buffers::{ArcPrivmsgsBuffer, SeenMsgIds},
    crypto::{decrypt_privmsg, decrypt_target},
    ChannelInfo, Privmsg, MAXIMUM_LENGTH_OF_MESSAGE,
};

mod command;

pub struct IrcServerConnection<C: AsyncRead + AsyncWrite + Send + Unpin + 'static> {
    // server stream
    write_stream: WriteHalf<C>,
    peer_address: SocketAddr,
    // msg ids
    seen_msg_ids: SeenMsgIds,
    privmsgs_buffer: ArcPrivmsgsBuffer,
    // user & channels
    is_nick_init: bool,
    is_user_init: bool,
    is_registered: bool,
    is_cap_end: bool,
    is_pass_init: bool,
    nickname: String,
    auto_channels: Vec<String>,
    pub configured_chans: FxHashMap<String, ChannelInfo>,
    pub configured_contacts: FxHashMap<String, crypto_box::SalsaBox>,
    capabilities: FxHashMap<String, bool>,
    // p2p
    p2p: P2pPtr,
    senders: SubscriberPtr<Privmsg>,
    subscriber_id: u64,
    password: String,
}

impl<C: AsyncRead + AsyncWrite + Send + Unpin + 'static> IrcServerConnection<C> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        write_stream: WriteHalf<C>,
        peer_address: SocketAddr,
        seen_msg_ids: SeenMsgIds,
        privmsgs_buffer: ArcPrivmsgsBuffer,
        auto_channels: Vec<String>,
        password: String,
        configured_chans: FxHashMap<String, ChannelInfo>,
        configured_contacts: FxHashMap<String, crypto_box::SalsaBox>,
        p2p: P2pPtr,
        senders: SubscriberPtr<Privmsg>,
        subscriber_id: u64,
    ) -> Self {
        let mut capabilities = FxHashMap::default();
        capabilities.insert("no-history".to_string(), false);
        Self {
            write_stream,
            peer_address,
            seen_msg_ids,
            privmsgs_buffer,
            is_nick_init: false,
            is_user_init: false,
            is_registered: false,
            is_cap_end: true,
            is_pass_init: false,
            nickname: "anon".to_string(),
            auto_channels,
            password,
            configured_chans,
            configured_contacts,
            capabilities,
            p2p,
            senders,
            subscriber_id,
        }
    }

    pub async fn process_msg_from_p2p(&mut self, msg: &Privmsg) -> Result<()> {
        info!("Received msg from P2p network: {:?}", msg);

        let mut msg = msg.clone();
        decrypt_target(&mut msg, self.configured_chans.clone(), self.configured_contacts.clone());

        if msg.target.starts_with('#') {
            // Try to potentially decrypt the incoming message.
            if !self.configured_chans.contains_key(&msg.target) {
                return Ok(())
            }

            let chan_info = self.configured_chans.get_mut(&msg.target).unwrap();
            if !chan_info.joined {
                return Ok(())
            }

            if let Some(salt_box) = &chan_info.salt_box {
                decrypt_privmsg(salt_box, &mut msg);
                info!("Decrypted received message: {:?}", msg);
            }

            // add the nickname to the channel's names
            if !chan_info.names.contains(&msg.nickname) {
                chan_info.names.push(msg.nickname.clone());
            }

            self.reply(&msg.to_string()).await?;
            return Ok(())
        } else if self.is_cap_end && self.is_nick_init && self.nickname == msg.target {
            if self.configured_contacts.contains_key(&msg.target) {
                let salt_box = self.configured_contacts.get(&msg.target).unwrap();
                decrypt_privmsg(salt_box, &mut msg);
                info!("Decrypted received message: {:?}", msg);
            }

            self.reply(&msg.to_string()).await?;
        }

        Ok(())
    }

    pub async fn process_line_from_client(
        &mut self,
        err: std::result::Result<usize, std::io::Error>,
        line: String,
    ) -> Result<()> {
        if let Err(e) = err {
            warn!("Read line error {}: {}", self.peer_address, e);
            return Err(Error::ChannelStopped)
        }

        info!("Received msg from IRC client: {:?}", line);
        let irc_msg = clean_input_line(line, &self.peer_address)?;

        if let Err(e) = self.update(irc_msg).await {
            warn!("Connection error: {} for {}", e, self.peer_address);
            return Err(Error::ChannelStopped)
        }
        Ok(())
    }

    async fn update(&mut self, line: String) -> Result<()> {
        if line.len() > MAXIMUM_LENGTH_OF_MESSAGE {
            return Err(Error::MalformedPacket)
        }

        if self.password.is_empty() {
            self.is_pass_init = true
        }

        let (command, value) = parse_line(&line)?;
        let (command, value) = (command.as_str(), value.as_str());
        info!("IRC server received command: {}", command);

        match command {
            "PASS" => self.on_receive_pass(value).await?,
            "USER" => self.on_receive_user().await?,
            "NAMES" => self.on_receive_names(value.split(',').map(String::from).collect()).await?,
            "NICK" => self.on_receive_nick(value).await?,
            "JOIN" => self.on_receive_join(value.split(',').map(String::from).collect()).await?,
            "PART" => self.on_receive_part(value.split(',').map(String::from).collect()).await?,
            "TOPIC" => self.on_receive_topic(&line, value).await?,
            "PING" => self.on_ping(value).await?,
            "PRIVMSG" => self.on_receive_privmsg(&line, value).await?,
            "CAP" => self.on_receive_cap(&line, &value.to_uppercase()).await?,
            "QUIT" => self.on_quit()?,
            _ => warn!("Unimplemented `{}` command", command),
        }

        self.registre().await?;
        Ok(())
    }

    async fn registre(&mut self) -> Result<()> {
        if !self.is_registered && self.is_cap_end && self.is_nick_init && self.is_user_init {
            debug!("Initializing peer connection");
            let register_reply = format!(":darkfi 001 {} :Let there be dark\r\n", self.nickname);
            self.reply(&register_reply).await?;
            self.is_registered = true;

            self.on_receive_join(self.auto_channels.clone()).await?;

            if *self.capabilities.get("no-history").unwrap() {
                return Ok(())
            }

            // Send dm messages in buffer
            let mut privmsgs_buffer = self.privmsgs_buffer.lock().await;
            privmsgs_buffer.update();
            for msg in privmsgs_buffer.iter() {
                let is_dm = msg.target == self.nickname ||
                    (msg.nickname == self.nickname && !msg.target.starts_with('#'));

                if is_dm {
                    self.senders.notify_by_id(msg.clone(), self.subscriber_id).await;
                }
            }
            drop(privmsgs_buffer);
        }
        Ok(())
    }

    async fn reply(&mut self, message: &str) -> Result<()> {
        self.write_stream.write_all(message.as_bytes()).await?;
        debug!("Sent {}", message);
        Ok(())
    }
}

//
// Helper functions
//

fn clean_input_line(mut line: String, peer_address: &SocketAddr) -> Result<String> {
    if line.is_empty() {
        warn!("Received empty line from {}. ", peer_address);
        warn!("Closing connection.");
        return Err(Error::ChannelStopped)
    }

    if line == "\n" || line == "\r\n" {
        warn!("Closing connection.");
        return Err(Error::ChannelStopped)
    }

    if &line[(line.len() - 2)..] == "\r\n" {
        // Remove CRLF
        line.pop();
        line.pop();
    } else if &line[(line.len() - 1)..] == "\n" {
        line.pop();
    } else {
        warn!("Closing connection.");
        return Err(Error::ChannelStopped)
    }

    Ok(line.clone())
}

fn parse_line(line: &str) -> Result<(String, String)> {
    let mut tokens = line.split_ascii_whitespace();
    // Commands can begin with :garbage but we will reject clients doing
    // that for now to keep the protocol simple and focused.
    let command = tokens.next().ok_or(Error::MalformedPacket)?.to_uppercase();
    let value = tokens.next().ok_or(Error::MalformedPacket)?;
    Ok((command.to_owned(), value.to_owned()))
}
