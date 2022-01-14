use async_std::sync::{Arc, Mutex};
use std::io;

use async_executor::Executor;
use log::*;

use crate::{
    blockchain::{rocks::columns, RocksColumn, Slab, SlabStore},
    net,
    net::{P2p, P2pPtr, Settings},
    util::{
        serial::{deserialize, serialize, Decodable, Encodable},
        sleep,
    },
    Error, Result,
};

pub struct Gateway {
    p2p: P2pPtr,
    slabstore: Arc<SlabStore>,
    last_indexes: Arc<Mutex<Vec<u64>>>,
}

impl Gateway {
    pub fn new(_settings: Settings, rocks: RocksColumn<columns::Slabs>) -> Result<Self> {
        let slabstore = SlabStore::new(rocks)?;
        let settings = Settings::default();

        let p2p = P2p::new(settings);
        let last_indexes = Arc::new(Mutex::new(vec![0; 10]));
        Ok(Self { p2p, slabstore, last_indexes })
    }

    pub async fn start(&self, executor: Arc<Executor<'_>>) -> Result<()> {
        self.p2p.clone().start(executor.clone()).await?;

        self.p2p.clone().run(executor.clone()).await?;

        Ok(())
    }

    async fn publish(&self, msg: GatewayMessage) -> Result<()> {
        self.p2p.broadcast(msg).await
    }

    async fn subscribe_loop(&self, executor: Arc<Executor<'_>>) -> Result<()> {
        let new_channel_sub = self.p2p.subscribe_channel().await;

        loop {
            let channel = new_channel_sub.receive().await?;

            let message_subsytem = channel.get_message_subsystem();

            message_subsytem.add_dispatch::<GatewayMessage>().await;

            let msg_sub = channel.subscribe_msg::<GatewayMessage>().await?;

            let jobsman = net::ProtocolJobsManager::new("GatewayMessage", channel);

            jobsman.clone().start(executor.clone());

            jobsman
                .spawn(Self::handle_msg(self.slabstore.clone(), msg_sub), executor.clone())
                .await;
        }
    }

    pub async fn handle_msg(
        slabstore: Arc<SlabStore>,
        msg_sub: net::MessageSubscription<GatewayMessage>,
    ) -> Result<()> {
        loop {
            let msg = msg_sub.receive().await?;

            match msg.get_command() {
                GatewayCommand::PutSlab => {
                    debug!(target: "GATEWAY", "Received putslab msg");

                    let slab = msg.get_payload();

                    slabstore.put(deserialize(&slab)?)?;

                    // TODO publish the new received slab
                }
                GatewayCommand::GetSlab => {
                    debug!(target: "GATEWAY", "Received getslab msg");

                    let index = msg.get_payload();
                    let _slab = slabstore.get(index)?;

                    // TODO publish the slab
                }
                GatewayCommand::GetLastIndex => {
                    debug!(target: "GATEWAY","Received getlastindex msg");

                    let _index = slabstore.get_last_index_as_bytes()?;

                    // TODO publish the inex
                }
            }
        }
    }

    pub async fn sync(&self) -> Result<()> {
        debug!(target: "GATEWAY", "Start Syncing");

        loop {
            let local_last_index = self.slabstore.get_last_index()?;

            // start syncing every 4 seconds
            sleep(4).await;

            self.get_last_index().await?;
            let last_index = 0;

            if last_index < local_last_index {
                return Err(Error::SlabsStore(
                    "Local slabstore has higher index than gateway's slabstore.
                 Run \" darkfid -r \" to refresh the database."
                        .into(),
                ))
            }

            if last_index > 0 {
                for index in (local_last_index + 1)..(last_index + 1) {
                    self.get_slab(index).await?
                }
            }

            debug!(target: "GATEWAY","End Syncing");
        }
    }

    pub async fn get_slab(&self, index: u64) -> Result<()> {
        debug!(target: "GATEWAY","Send get slab msg");
        let msg = GatewayMessage::new(GatewayCommand::GetSlab, serialize(&index));
        self.publish(msg).await
    }

    pub async fn put_slab(&self, slab: Slab) -> Result<()> {
        debug!(target: "GATEWAY","Send put slab msg");
        let msg = GatewayMessage::new(GatewayCommand::PutSlab, serialize(&slab));
        self.publish(msg).await
    }

    pub async fn get_last_index(&self) -> Result<()> {
        debug!(target: "GATEWAY","Send get last index msg");
        let msg = GatewayMessage::new(GatewayCommand::PutSlab, vec![]);
        self.publish(msg).await
    }

    pub fn get_slabstore(&self) -> Arc<SlabStore> {
        self.slabstore.clone()
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum GatewayCommand {
    PutSlab,
    GetSlab,
    GetLastIndex,
}

#[derive(Debug, PartialEq, Clone)]
pub struct GatewayMessage {
    command: GatewayCommand,
    payload: Vec<u8>,
}

impl GatewayMessage {
    pub fn new(command: GatewayCommand, payload: Vec<u8>) -> Self {
        Self { command, payload }
    }
    pub fn get_command(&self) -> GatewayCommand {
        self.command.clone()
    }

    pub fn get_payload(&self) -> Vec<u8> {
        self.payload.clone()
    }
}

impl Encodable for GatewayMessage {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += (self.command.clone() as u8).encode(&mut s)?;
        len += self.payload.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for GatewayMessage {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        let command_code: u8 = Decodable::decode(&mut d)?;
        let command = match command_code {
            0 => GatewayCommand::PutSlab,
            1 => GatewayCommand::GetSlab,
            _ => GatewayCommand::GetLastIndex,
        };

        Ok(Self { command, payload: Decodable::decode(&mut d)? })
    }
}

impl net::Message for GatewayMessage {
    fn name() -> &'static str {
        "reply"
    }
}
