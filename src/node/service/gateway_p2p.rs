use std::sync::Arc;
use std::io;

use rand::Rng;
use async_executor::Executor;
use log::*;

use crate::{
    blockchain::{rocks::columns, RocksColumn, Slab, SlabStore},
    util::serial::{Decodable, Encodable},
    Error, Result,
    net::{P2pPtr, P2p, Settings}
};

#[allow(dead_code)]
pub struct Gateway {
    p2p: P2pPtr,
    slabstore: Arc<SlabStore>,
}

impl Gateway {
    pub fn new(
        _settings: Settings,
        rocks: RocksColumn<columns::Slabs>,
    ) -> Result<Self> {
        let slabstore = SlabStore::new(rocks)?;
        let settings = Settings::default();

        let p2p = P2p::new(settings);
        Ok(Self { p2p, slabstore})
    }

    pub async fn start(&self, executor: Arc<Executor<'_>>) -> Result<()> {

        self.p2p.clone().start(executor.clone()).await?;

        self.p2p.clone().run(executor.clone()).await?;

        Ok(())
    }


    async fn publish(&self, _msg: GatewayMessage) -> Result<()> {
        Ok(())
    }

    async fn subscribe_loop(&self, _executor: Arc<Executor<'_>>) -> Result<()> {
        let new_channel_sub = self.p2p.subscribe_channel().await;

        loop {
            let channel = new_channel_sub.receive().await?;
            // do something
        }

    }

    async fn handle_msg(
        msg: GatewayMessage,
        _slabstore: Arc<SlabStore>,
    ) -> Result<()> {
        match msg.get_command() {
            GatewayCommand::PutSlab => {
                debug!(target: "GATEWAY DAEMON", "Received putslab msg");
            }
            GatewayCommand::GetSlab => {
                debug!(target: "GATEWAY DAEMON", "Received getslab msg");
            }
            GatewayCommand::GetLastIndex => {
                debug!(target: "GATEWAY DAEMON","Received getlastindex msg");
            }
        }
        Ok(())
    }

    pub async fn sync(&mut self) -> Result<u64> {
        debug!(target: "GATEWAY CLIENT", "Start Syncing");
        Ok(0)
    }

    pub async fn get_slab(&mut self, _index: u64) -> Result<Option<Slab>> {
        debug!(target: "GATEWAY CLIENT","Get slab");
        Ok(None)
    }

    pub async fn put_slab(&mut self, _slab: Slab) -> Result<()> {
        debug!(target: "GATEWAY CLIENT","Put slab");
        Ok(())
    }

    pub async fn get_last_index(&self) -> Result<u64> {
        debug!(target: "GATEWAY CLIENT","Get last index");
        Ok(0)
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
    id: u32,
    payload: Vec<u8>,
}

impl GatewayMessage {
    pub fn new(command: GatewayCommand, payload: Vec<u8>) -> Self {
        let id = Self::gen_id();
        Self { command, id, payload }
    }
    fn gen_id() -> u32 {
        let mut rng = rand::thread_rng();
        rng.gen()
    }

    pub fn get_id(&self) -> u32 {
        self.id
    }

    pub fn get_command(&self) ->  GatewayCommand{
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
        len += self.id.encode(&mut s)?;
        len += self.payload.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for GatewayMessage {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        let command_code: u8 =  Decodable::decode(&mut d)?;
        let command = match  command_code {
            0 => GatewayCommand::GetSlab,
            1 => GatewayCommand::PutSlab,
            2 => GatewayCommand::GetLastIndex,
            _ => GatewayCommand::GetLastIndex,
        };

        Ok(Self {
            command,
            id: Decodable::decode(&mut d)?,
            payload: Decodable::decode(&mut d)?,
        })
    }
}

impl crate::net::Message for GatewayMessage {
    fn name() -> &'static str {
        "reply"
    }
}

