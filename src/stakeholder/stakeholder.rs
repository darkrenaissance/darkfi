use async_executor::Executor;
use async_trait::async_trait;
use async_std::sync::Arc;
use log::debug;
use std::fmt;

use std::time::Duration;
use std::thread;

use crate::zk::circuit::LeadContract;

use crate::{
    consensus::{Block, BlockInfo,Metadata,StreamletMetadata,TransactionLeadProof},
    util::{
        time::Timestamp,
        clock::{Clock,Ticks},
        expand_path,
    },
    system::{Subscriber, SubscriberPtr, Subscription},
    crypto::{
        proof::{Proof, ProvingKey, VerifyingKey,  },
        leadcoin::{LeadCoin},
    },
    blockchain::{Blockchain,Epoch,EpochConsensus},
    net::{P2p,Settings, SettingsPtr, Channel, ChannelPtr, Hosts, HostsPtr,MessageSubscription},
    tx::{Transaction},
    Result,Error,
};

use url::Url;

use pasta_curves::{
    pallas,
};

use group::ff::PrimeField;

#[derive(Debug)]
pub struct SlotWorkspace
{
    pub st : blake3::Hash,
    pub e: u64,
    pub sl: u64,
    pub txs: Vec<Transaction>,
    pub metadata: Metadata,
    pub is_leader: bool,
    pub proof: Proof,
    pub block: BlockInfo,
}

impl Default for SlotWorkspace {
    fn default() -> Self {
        Self {st: blake3::hash(b""),
              e: 0,
              sl: 0,
              txs: vec![],
              is_leader: false,
              metadata: Metadata::default(),
              proof: Proof::default(),
              block: BlockInfo::default(),
        }
    }
}

impl SlotWorkspace {

    pub fn new_block(&self) -> (BlockInfo, blake3::Hash) {
        let sm = StreamletMetadata::new(vec!());
        let block = BlockInfo::new(self.st, self.e, self.sl, self.txs.clone(), self.metadata.clone(), sm);
        let hash = block.blockhash();
        (block, hash)
    }

    pub fn add_tx(& mut self, tx: Transaction) {
        self.txs.push(tx);
    }

    pub fn set_metadata(& mut self, md : Metadata) {
        self.metadata = md;
    }

    pub fn set_sl(&mut self, sl: u64) {
        self.sl = sl;
    }

    pub fn set_st(&mut self, st: blake3::Hash) {
        self.st = st;
    }


    pub fn set_e(&mut self, e: u64) {
        self.e = e;
    }

    pub fn set_proof(&mut self, proof: Proof) {
        self.proof = proof;
    }

    pub fn set_leader(&mut self, alead : bool) {
        self.is_leader = alead;
    }
}


pub struct Stakeholder
{
    pub blockchain: Blockchain, // stakeholder view of the blockchain
    pub net : Arc<P2p>,
    pub clock : Clock,
    pub coins : Vec<LeadCoin>, // owned stakes
    pub epoch : Epoch, // current epoch
    pub epoch_consensus : EpochConsensus, // configuration for the epoch
    pub pk : ProvingKey,
    pub vk : VerifyingKey,
    pub playing: bool,
    pub workspace : SlotWorkspace,
    pub id: u8,
    //pub subscription: Subscription<Result<ChannelPtr>>,
    //pub chanptr : ChannelPtr,
    //pub msgsub : MessageSubscription::<BlockInfo>,
}

impl Stakeholder
{
    pub async fn new(consensus: EpochConsensus, settings: Settings, rel_path: &str, id: u8, k: Option<u32>) -> Result<Self>
    {
        let path = expand_path(&rel_path).unwrap();
        debug!("opening db");
        let db = sled::open(&path)?;
        debug!("opend db");
        let ts = Timestamp::current_time();
        let genesis_hash = blake3::hash(b"");
        //TODO lisen and add transactions

        let bc = Blockchain::new(&db, ts, genesis_hash).unwrap();

        //TODO replace with const
        let eta = pallas::Base::one();
        let epoch = Epoch::new(consensus, eta);

        let lead_pk = ProvingKey::build(k.unwrap(), &LeadContract::default());
        let lead_vk = VerifyingKey::build(k.unwrap(), &LeadContract::default());
        let p2p = P2p::new(settings.clone()).await;

        //TODO
        let workspace = SlotWorkspace::default();

        //
        let clock = Clock::new(Some(consensus.get_epoch_len()), Some(consensus.get_slot_len()), Some(consensus.get_tick_len()), settings.peers);
        debug!("stakeholder constructed...");
        Ok(Self{blockchain: bc,
                net: p2p,
                clock: clock,
                coins: vec![], //constructed with empty coins for sake of simulation only
                // but should be populated from wallet db.
                epoch: epoch,
                epoch_consensus: consensus,
                pk: lead_pk,
                vk: lead_vk,
                playing: true,
                workspace: workspace,
                id: id,
                //subscription: subscription,
                //chanptr: chanptr,
                //msgsub: msg_sub,
        })
    }

    pub fn get_provkingkey(&self) -> ProvingKey {
        self.pk.clone()
    }

    pub fn get_verifyingkey(&self) -> VerifyingKey {
        self.vk.clone()
    }

    /// get list stakeholder peers on the p2p network for synchronization
    pub fn get_peers(&self) -> Vec<Url> {
        let settings : SettingsPtr = self.net.settings();
        settings.peers.clone()
    }

    /*
    fn  new_block(&self) {
        //TODO initialize blocks in the epoch, and add coin commitment in genesis
        let block_info = BlockInfo::new(st, e, sl, txs, metadata, sm);
        self.block = block_info;
    }
    */

    fn init_network(&self) -> Result<()>{
        //TODO initialize exectutor
        //let exec = Arc<Executor<'_>>;
        let exec = Arc::new(Executor::new());
        exec.run(self.net.clone().start(exec.clone()));
        //self.net(exec);

        Ok(())
    }

    pub fn get_net(&self) -> Arc<P2p> {
        //TODO use P2p ptr not to overwrite wrappers
        self.net.clone()
    }

    /// add new blockinfo to the blockchain
    pub fn add_block(&self, block: BlockInfo) {
        let blocks = [block];
        self.blockchain.add(&blocks);
    }

    pub fn add_tx(&mut self, tx: Transaction)
    {
        self.workspace.add_tx(tx);
    }

    /// extract leader selection lottery randomness \eta
    /// it's the hash of the previous lead proof
    /// converted to pallas base
    pub fn get_eta(&self) -> pallas::Base
    {
        let last_proof_slot : u64 = 0;
        let proof_tx_hash = self.blockchain.get_last_proof_hash().unwrap();
        let mut bytes : [u8;32] = *proof_tx_hash.as_bytes();
        // read first 254 bits
        bytes[30] = 0;
        bytes[31] = 0;
        pallas::Base::from_repr(bytes).unwrap()
    }

    pub fn valid_block(&self, blk : BlockInfo)  -> bool {
        //TODO implement
        true
    }

    /// listen to the network,
    /// for new transactions.
    pub fn sync_tx (&self) {
        //TODO
    }

    /// listen to the network channels,
    /// receive new messages, or blocks,
    /// validate the block proof, and the transactions,
    /// if so add the proof to metadata if stakeholder isn't the lead.
    pub async fn sync_block(&self) {
        let subscription : Subscription<Result<ChannelPtr>> = self.net.subscribe_channel().await;
        debug!("--> channel");
        let chanptr : ChannelPtr =  subscription.receive().await.unwrap();
        debug!("--> received channel");
        //
        let message_subsytem = chanptr.get_message_subsystem();
        debug!("--> adding dispatcher to msg subsystem");
        message_subsytem.add_dispatch::<BlockInfo>().await;
        debug!("--> added");
        //TODO start channel if isn't started yet
        //let info = chanptr.get_info();
        //debug!("channel info: {}", info);
        debug!("--> subscribe msg_sub");
        let msg_sub : MessageSubscription::<BlockInfo> =
            chanptr.subscribe_msg::<BlockInfo>().await.expect("missing blockinfo");
        debug!("--> subscribed");

        let res = msg_sub.receive().await.unwrap();
        let blk : BlockInfo = (*res).to_owned();
        //TODO validate the block proof, and transactions.
        if self.valid_block(blk.clone())  {
            //TODO if valid only.
            self.blockchain.add(&[blk.clone()]);
        } else {
            debug!("received block is invalid!");
        }
    }

    pub async fn background(&mut self) {
        self.clock.sync().await;
        while self.playing {
            // clock ticks slot begins
            // initialize the epoch if it's the time
            // check for leadership
            match self.clock.ticks().await {
                Ticks::GENESIS{e, sl} => {
                    //TODO (res) any initialization happening here?
                    self.new_epoch();
                    self.new_slot(e, sl);
                }
                Ticks::NEWEPOCH{e, sl} => {
                    self.new_epoch();
                    self.new_slot(e, sl);
                }
                Ticks::NEWSLOT{e, sl} => self.new_slot(e, sl),
                Ticks::TOCKS => {
                    // slot is about to end.
                    // sync, and validate.
                    // no more transactions to be received/send to the end of slot.
                    if self.workspace.is_leader {
                        debug!("<<<--- [[[leadership won]]] --->>>");
                        //craete block
                        let (block_info, block_hash) = self.workspace.new_block();
                        //add the block to the blockchain
                        self.add_block(block_info.clone());
                        let block : Block = Block::from(block_info.clone());
                        // publish the block.
                        self.net.broadcast(block);
                    } else {
                        //
                        self.sync_block();
                    }
                },
                Ticks::IDLE => {
                    continue
                }
                Ticks::OUTOFSYNC => {
                    // clock, and blockchain are out of sync
                    self.clock.sync().await;
                    self.sync_block();
                }
            }
            thread::sleep(Duration::from_millis(1000));
        }
    }

    pub fn to_string(&self) -> String {
        format!("stakeholder with id:{}", self.id.to_string())
    }
    /// on the onset of the epoch, layout the new the competing coins
    /// assuming static stake during the epoch, enforced by the commitment to competing coins
    /// in the epoch's gen2esis data.
    fn new_epoch(&mut self)
    {
        debug!("[new epoch] 4 {}", self);
        let eta = self.get_eta();
        let mut epoch = Epoch::new(self.epoch_consensus, self.get_eta());
        epoch.create_coins(); // set epoch interal fields working space with competing coins
        self.epoch = epoch.clone();
    }


    /// at the begining of the slot
    /// stakeholder need to play the lottery for the slot.
    /// FIXME if the stakeholder is not winning, staker can try different coins before,
    /// commiting it's coins, to maximize success, thus,
    /// the lottery proof need to be conditioned on the slot itself, and previous proof.
    /// this will encourage each potential leader to play with honesty.
    fn new_slot(&mut self, e: u64, sl: u64)
    {
        debug!("[new slot] 4 {}\ne:{}, sl:{}", self, e, sl);
        let EMPTY_PTR = blake3::hash(b"");
        let st : blake3::Hash = if e>0 || (e==0&&sl>0) {
            self.workspace.block.blockhash()
        } else {
            EMPTY_PTR
        };
        let is_leader : bool = self.epoch.is_leader(sl);
        // if is leader create proof
        let proof = if is_leader {
            self.epoch.get_proof(sl, &self.pk.clone())
        } else {
            Proof::new(vec![])
        };
        // set workspace
        self.workspace.set_sl(sl);
        self.workspace.set_e(e);
        self.workspace.set_st(st);
        self.workspace.set_leader(is_leader);
        self.workspace.set_proof(proof.clone());
        //
        if is_leader {
            let metadata = Metadata::new(Timestamp::current_time(),
                                         self.get_eta().to_repr(),
                                         TransactionLeadProof::from(proof.clone()));
            self.workspace.set_metadata(metadata);
        }
    }
}

impl fmt::Display for Stakeholder {
    fn fmt(&self, formater : &mut fmt::Formatter) ->  fmt::Result {
        formater.write_fmt(format_args!("stakeholder with id: {}", self.id))
    }
}
