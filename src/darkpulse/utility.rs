use std::{
    fs::OpenOptions,
    io::prelude::*,
    net::SocketAddr,
    path::PathBuf,
    sync::{atomic::AtomicU64, Arc},
    time::{SystemTime, UNIX_EPOCH},
};

use async_executor::Executor;
use futures::prelude::*;
use log::*;

use super::{
    aes_encrypt, messages, Channel, ControlCommand, ControlMessage, Dbsql, MessagePayload,
    ProtocolSlab, SlabsManagerSafe,
};

use crate::{
    net::ChannelPtr,
    serial::{deserialize, serialize},
    Result,
};

pub type AddrsStorage = Arc<async_std::sync::Mutex<Vec<SocketAddr>>>;

pub type Clock = Arc<AtomicU64>;

pub fn get_current_time() -> u64 {
    let start = SystemTime::now();
    let since_the_epoch =
        start.duration_since(UNIX_EPOCH).expect("Incorrect system clock: time went backwards");
    let in_ms =
        since_the_epoch.as_secs() * 1000 + since_the_epoch.subsec_nanos() as u64 / 1_000_000;
    return in_ms
}

pub fn save_to_addrs_store(stored_addrs: &Vec<SocketAddr>) -> Result<()> {
    let path = default_config_dir()?.join("addrs.add");
    let mut writer = OpenOptions::new().write(true).create(true).open(path)?;
    let buffer = serialize(stored_addrs);
    writer.write_all(&buffer)?;
    Ok(())
}

pub fn default_config_dir() -> Result<PathBuf> {
    let mut path = PathBuf::new();

    if let Some(home_dir) = dirs::home_dir() {
        path = home_dir;
    };

    let path = path.join(".darkpulse/");
    if !path.exists() {
        match std::fs::create_dir(&path) {
            Err(err) => {
                eprintln!("error: Creating config dir: {}", err);
                std::process::exit(-1);
            }
            Ok(()) => (),
        }
    }

    Ok(path)
}

pub fn load_stored_addrs() -> Result<Vec<SocketAddr>> {
    let path = default_config_dir()?.join("addrs.add");
    println!("{:?}", path);
    let mut reader = OpenOptions::new().read(true).write(true).create(true).open(path)?;
    let mut buffer = Vec::new();
    reader.read_to_end(&mut buffer)?;
    if !buffer.is_empty() {
        let addrs: Vec<SocketAddr> = deserialize(&buffer)?;
        Ok(addrs)
    } else {
        Ok(vec![])
    }
}

pub async fn pack_slab(
    channel_secret: &[u8; 32],
    username: String,
    message: String,
    control_command: ControlCommand,
) -> Result<messages::SlabMessage> {
    let nonce: [u8; 12] = rand::random();
    let timestamp = chrono::offset::Utc::now();
    let timestamp: i64 = timestamp.timestamp_millis() / 1000;

    let msg_payload = MessagePayload { nickname: username.clone(), text: message, timestamp };

    let control_message = ControlMessage { control: control_command, payload: msg_payload };

    let ser_message = serialize(&control_message);

    let ciphertext = aes_encrypt(channel_secret, &nonce, &ser_message[..])
        .expect("error during encrypting the message");

    let slab = messages::SlabMessage { nonce, ciphertext };

    Ok(slab)
}

pub fn setup_username(newname: Option<String>, db: &Dbsql) -> Result<String> {
    let mut _username: String = String::new();
    match newname {
        Some(nm) => {
            _username = nm.clone();
            db.add_username(&nm).unwrap();
        }
        None => {
            _username = db.get_username()?;
            if _username.is_empty() {
                _username = String::from("username");
            }
        }
    }
    Ok(_username)
}

pub async fn read_line<R: AsyncBufRead + Unpin>(reader: &mut R) -> Result<String> {
    let mut buf = String::new();
    let _ = reader.read_line(&mut buf).await?;
    Ok(buf.trim().to_string())
}

pub fn choose_channel(db: &Dbsql, channel_name: Option<String>) -> Result<Channel> {
    let channels = db.get_channels()?;
    let mut main_channel = Channel::gen_new(String::from("test_channel"));
    if channels.len() > 0 {
        match channel_name {
            Some(name) => {
                main_channel = channels
                    .iter()
                    .filter(|ch| ch.get_channel_name() == &name)
                    .next()
                    .expect(format!("there is no channel with the name {}: ", name).as_str())
                    .clone();
            }
            None => {
                main_channel = channels.first().unwrap().clone();
            }
        }
    } else {
        error!("there are no channels available");
        db.add_channel(&main_channel)?;
    }
    Ok(main_channel)
}

pub async fn setup_network_channel(
    executor: Arc<Executor<'_>>,
    channel: ChannelPtr,
    slabman: SlabsManagerSafe,
) {
    let message_subsytem = channel.get_message_subsystem();

    message_subsytem.add_dispatch::<messages::SyncMessage>().await;
    message_subsytem.add_dispatch::<messages::InvMessage>().await;
    message_subsytem.add_dispatch::<messages::GetSlabsMessage>().await;
    message_subsytem.add_dispatch::<messages::SlabMessage>().await;

    let protocol_slab = ProtocolSlab::new(slabman, channel.clone()).await;
    protocol_slab.clone().start(executor.clone()).await;
}
