use std::sync::Arc;

use log::*;
use smol::Executor;

use crate::{
    darkpulse::{
        aes_decrypt, messages, CiphertextHash, ControlCommand, ControlMessage, SlabsManagerSafe,
    },
    error::Result as NetResult,
    net::{
        message_subscriber::MessageSubscription,
        protocols::{ProtocolJobsManager, ProtocolJobsManagerPtr},
        ChannelPtr,
    },
    serial::deserialize,
};

pub struct ProtocolSlab {
    channel: ChannelPtr,
    slabman: SlabsManagerSafe,

    sync_sub: MessageSubscription<messages::SyncMessage>,
    inv_sub: MessageSubscription<messages::InvMessage>,
    get_slabs_sub: MessageSubscription<messages::GetSlabsMessage>,
    slab_sub: MessageSubscription<messages::SlabMessage>,

    jobsman: ProtocolJobsManagerPtr,
}

impl ProtocolSlab {
    pub async fn new(slabman: SlabsManagerSafe, channel: ChannelPtr) -> Arc<Self> {
        let sync_sub = channel
            .clone()
            .subscribe_msg::<messages::SyncMessage>()
            .await
            .expect("Missing sync  dispatcher!");

        let inv_sub = channel
            .clone()
            .subscribe_msg::<messages::InvMessage>()
            .await
            .expect("Missing inv  dispatcher!");

        let get_slabs_sub = channel
            .clone()
            .subscribe_msg::<messages::GetSlabsMessage>()
            .await
            .expect("Missing getslabs  dispatcher!");

        let slab_sub = channel
            .clone()
            .subscribe_msg::<messages::SlabMessage>()
            .await
            .expect("Missing slab  dispatcher!");

        Arc::new(Self {
            channel: channel.clone(),
            slabman,
            sync_sub,
            inv_sub,
            get_slabs_sub,
            slab_sub,
            jobsman: ProtocolJobsManager::new("ProtocolSlab", channel),
        })
    }

    pub async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) {
        debug!(target: "net", "ProtocolSlab::start() [START]");
        self.jobsman.clone().start(executor.clone());

        self.jobsman.clone().spawn(self.clone().handle_receive_sync(), executor.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_receive_inv(), executor.clone()).await;

        self.jobsman.clone().spawn(self.clone().handle_receive_get_slabs(), executor.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_receive_slab(), executor).await;

        let _ = self.channel.send(messages::SyncMessage {}).await;

        debug!(target: "net", "ProtocolSlab::start() [END]");
    }

    async fn handle_receive_sync(self: Arc<Self>) -> NetResult<()> {
        debug!(target: "net", "ProtocolSlab::handle_receive_sync() [START]");
        loop {
            let _sync_msg = self.sync_sub.receive().await?;
            let slab_hashs = self.slabman.lock().await.get_slabs_hash();
            let inv_msg = messages::InvMessage { slabs_hash: slab_hashs.clone() };
            self.channel.send(inv_msg).await?;
            info!("receive sync message!");
        }
    }

    async fn handle_receive_inv(self: Arc<Self>) -> NetResult<()> {
        debug!(target: "net", "ProtocolSlab::handle_receive_inv() [START]");
        loop {
            let inv_msg = self.inv_sub.receive().await?;
            let mut list_of_hash: Vec<CiphertextHash> = vec![];
            let slabs_hash = self.slabman.lock().await.get_slabs_hash();
            for slab in inv_msg.slabs_hash.iter() {
                if !slabs_hash.contains(slab) {
                    list_of_hash.push(slab.clone());
                }
            }
            let getslabs_msg = messages::GetSlabsMessage { slabs_hash: list_of_hash };
            self.channel.send(getslabs_msg).await?;
            info!("receive inv message!");
        }
    }

    async fn handle_receive_get_slabs(self: Arc<Self>) -> NetResult<()> {
        debug!(target: "net", "ProtocolSlab::handle_receive_get_slabs() [START]");
        loop {
            let get_slabs_msg = self.get_slabs_sub.receive().await?;
            for slab_hash in get_slabs_msg.slabs_hash.iter() {
                let slabman = self.slabman.lock().await;
                let slab = slabman.get_slab(&slab_hash);
                if let Some(slab) = slab {
                    self.channel.send(slab.clone()).await?;
                }
            }
            info!("receive getslabs message!");
        }
    }

    async fn handle_receive_slab(self: Arc<Self>) -> NetResult<()> {
        debug!(target: "net", "ProtocolSlab::handle_receive_slab() [START]");
        loop {
            let slab_msg = self.slab_sub.receive().await?;
            info!("receive slab message!");

            let channels = self.slabman.lock().await.get_channels().unwrap_or(vec![]);

            let slab = messages::SlabMessage {
                nonce: slab_msg.nonce,
                ciphertext: slab_msg.ciphertext.clone(),
            };

            for channel in channels.iter() {
                match aes_decrypt(
                    &channel.get_channel_secret(),
                    &slab_msg.nonce,
                    &slab_msg.ciphertext,
                ) {
                    Some(plaintext) => {
                        self.slabman
                            .lock()
                            .await
                            .add_new_slab(slab.clone())
                            .await
                            .expect("error during adding new slab to database");

                        let des_plaintext: ControlMessage = deserialize(&plaintext[..])
                            .expect("error during deserializing the message");

                        match des_plaintext.control {
                            ControlCommand::Join => {
                                info!("{} joined the group", des_plaintext.payload.nickname);
                            }
                            ControlCommand::Leave => {
                                info!("{} left the group", des_plaintext.payload.nickname);
                            }
                            ControlCommand::Message => {
                                info!(
                                    "{} -> {}: {}",
                                    des_plaintext.payload.timestamp,
                                    des_plaintext.payload.nickname,
                                    des_plaintext.payload.text
                                );
                            }
                        }
                    }
                    None => {}
                }
            }
        }
    }
}
