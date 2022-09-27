use async_executor::Executor;
use async_std::sync::Arc;
use log::{debug, error, info};
use std::fmt;

use rand::rngs::OsRng;
use std::{thread, time::Duration};

use crate::zk::circuit::LeadContract;

use crate::{
    blockchain::{Blockchain, Epoch, EpochConsensus},
    consensus::{
        Block, BlockInfo, Header, OuroborosMetadata, StakeholderMetadata, StreamletMetadata,
        TransactionLeadProof,
    },
    crypto::{
        address::Address,
        keypair::Keypair,
        leadcoin::LeadCoin,
        merkle_node::MerkleNode,
        proof::{Proof, ProvingKey, VerifyingKey},
        schnorr::{SchnorrPublic, SchnorrSecret, Signature},
    },
    net::{MessageSubscription, P2p, Settings, SettingsPtr},
    tx::Transaction,
    util::{
        clock::{Clock, Ticks},
        path::expand_path,
        time::Timestamp,
    },
    Result,
};

use url::Url;

use pasta_curves::pallas;

use group::ff::PrimeField;

const LOG_T: &str = "stakeholder";

#[derive(Debug)]
pub struct SlotWorkspace {
    pub st: blake3::Hash,      // hash of the previous block
    pub e: u64,                // epoch index
    pub sl: u64,               // relative slot index
    pub txs: Vec<Transaction>, // unpublished block transactions
    pub root: MerkleNode,
    /// merkle root of txs
    pub m: StakeholderMetadata,
    pub om: OuroborosMetadata,
    pub is_leader: bool,
    pub proof: Proof,
    pub block: BlockInfo,
}

impl Default for SlotWorkspace {
    fn default() -> Self {
        Self {
            st: blake3::hash(b""),
            e: 0,
            sl: 0,
            txs: vec![],
            root: MerkleNode(pallas::Base::zero()),
            is_leader: false,
            m: StakeholderMetadata::default(),
            om: OuroborosMetadata::default(),
            proof: Proof::default(),
            block: BlockInfo::default(),
        }
    }
}

impl SlotWorkspace {
    pub fn new_block(&self) -> (BlockInfo, blake3::Hash) {
        let sm = StreamletMetadata::new(vec![]);
        let header = Header::new(self.st, self.e, self.sl, Timestamp::current_time(), self.root);
        let block = BlockInfo::new(header, self.txs.clone(), self.m.clone(), self.om.clone(), sm);
        let hash = block.blockhash();
        (block, hash)
    }

    pub fn add_tx(&mut self, tx: Transaction) {
        self.txs.push(tx);
    }

    pub fn set_root(&mut self, root: MerkleNode) {
        self.root = root;
    }

    pub fn set_stakeholdermetadata(&mut self, meta: StakeholderMetadata) {
        self.m = meta;
    }

    pub fn set_ouroborosmetadata(&mut self, meta: OuroborosMetadata) {
        self.om = meta;
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

    pub fn set_leader(&mut self, alead: bool) {
        self.is_leader = alead;
    }
}

pub struct Stakeholder {
    pub blockchain: Blockchain, // stakeholder view of the blockchain
    pub net: Arc<P2p>,
    pub clock: Clock,
    pub coins: Vec<LeadCoin>,            // owned stakes
    pub epoch: Epoch,                    // current epoch
    pub epoch_consensus: EpochConsensus, // configuration for the epoch
    pub pk: ProvingKey,
    pub vk: VerifyingKey,
    pub playing: bool,
    pub workspace: SlotWorkspace,
    pub id: i64,
    pub keypair: Keypair,
    //pub subscription: Subscription<Result<ChannelPtr>>,
    //pub chanptr : ChannelPtr,
    //pub msgsub : MessageSubscription::<BlockInfo>,
}

impl Stakeholder {
    pub async fn new(
        consensus: EpochConsensus,
        settings: Settings,
        rel_path: &str,
        id: i64,
        k: Option<u32>,
    ) -> Result<Self> {
        let path = expand_path(rel_path).unwrap();
        let db = sled::open(&path)?;
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
        let clock = Clock::new(
            Some(consensus.get_epoch_len()),
            Some(consensus.get_slot_len()),
            Some(consensus.get_tick_len()),
            settings.peers,
        );
        let keypair = Keypair::random(&mut OsRng);
        debug!(target: LOG_T, "stakeholder constructed");
        Ok(Self {
            blockchain: bc,
            net: p2p,
            clock,
            coins: vec![], //constructed with empty coins for sake of simulation only
            // but should be populated from wallet db.
            epoch,
            epoch_consensus: consensus,
            pk: lead_pk,
            vk: lead_vk,
            playing: true,
            workspace,
            id,
            keypair, //subscription: subscription,
                     //chanptr: chanptr,
                     //msgsub: msg_sub,
        })
    }

    /// wrapper on Schnorr signature
    pub fn sign(&self, message: &[u8]) -> Signature {
        self.keypair.secret.sign(message)
    }

    /// wrapper on schnorr public verify
    pub fn verify(&self, message: &[u8], signature: &Signature) -> bool {
        self.keypair.public.verify(message, signature)
    }

    pub fn get_provkingkey(&self) -> ProvingKey {
        self.pk.clone()
    }

    pub fn get_verifyingkey(&self) -> VerifyingKey {
        self.vk.clone()
    }

    /// get list stakeholder peers on the p2p network for synchronization
    pub fn get_peers(&self) -> Vec<Url> {
        let settings: SettingsPtr = self.net.settings();
        settings.peers.clone()
    }

    /*
    fn  new_block(&self) {
        //TODO initialize blocks in the epoch, and add coin commitment in genesis
        let block_info = BlockInfo::new(st, e, sl, txs, metadata, sm);
        self.block = block_info;
    }
    */

    async fn init_network(&self) -> Result<()> {
        let exec = Arc::new(Executor::new());
        self.net.clone().start(exec.clone()).await?;
        //TODO (fix) await blocks
        self.net.clone().run(exec);
        info!(target: LOG_T, "net initialized");
        Ok(())
    }

    pub fn get_net(&self) -> Arc<P2p> {
        //TODO use P2p ptr not to overwrite wrappers
        self.net.clone()
    }

    /// add new blockinfo to the blockchain
    pub fn add_block(&self, block: BlockInfo) {
        let blocks = [block];
        let _len = self.blockchain.add(&blocks);
    }

    pub fn add_tx(&mut self, tx: Transaction) {
        self.workspace.add_tx(tx);
    }

    /// extract leader selection lottery randomness \eta
    /// it's the hash of the previous lead proof
    /// converted to pallas base
    pub fn get_eta(&self) -> pallas::Base {
        let proof_tx_hash = self.blockchain.get_last_proof_hash().unwrap();
        let mut bytes: [u8; 32] = *proof_tx_hash.as_bytes();
        // read first 254 bits
        bytes[30] = 0;
        bytes[31] = 0;
        pallas::Base::from_repr(bytes).unwrap()
    }

    pub fn valid_block(&self, _blk: BlockInfo) -> bool {
        //TODO implement
        true
    }

    /// listen to the network,
    /// for new transactions.
    pub fn sync_tx(&self) {
        //TODO
    }

    /// listen to the network channels,
    /// receive new messages, or blocks,
    /// validate the block proof, and the transactions,
    /// if so add the proof to metadata if stakeholder isn't the lead.
    pub async fn sync_block(&self) {
        info!(target: LOG_T, "syncing blocks");
        for chanptr in self.net.channels().lock().await.values() {
            let message_subsytem = chanptr.get_message_subsystem();
            message_subsytem.add_dispatch::<BlockInfo>().await;
            //TODO start channel if isn't started yet
            //let info = chanptr.get_info();
            let msg_sub: MessageSubscription<BlockInfo> =
                chanptr.subscribe_msg::<BlockInfo>().await.expect("missing blockinfo");

            let res = msg_sub.receive().await.unwrap();
            let blk: BlockInfo = (*res).to_owned();
            //TODO validate the block proof, and transactions.
            if self.valid_block(blk.clone()) {
                //TODO if valid only.
                let _len = self.blockchain.add(&[blk]);
            } else {
                error!(target: LOG_T, "received block is invalid!");
            }
        }
    }

    pub async fn background(&mut self, hardlimit: Option<u8>) {
        let _ = self.init_network().await;
        let _ = self.clock.sync().await;
        let mut c: u8 = 0;
        let lim: u8 = hardlimit.unwrap_or(0);
        while self.playing {
            if c > lim && lim > 0 {
                break
            }
            // clock ticks slot begins
            // initialize the epoch if it's the time
            // check for leadership
            match self.clock.ticks().await {
                Ticks::GENESIS { e, sl } => {
                    //TODO (res) any initialization happening here?
                    self.new_epoch();
                    self.new_slot(e, sl);
                }
                Ticks::NEWEPOCH { e, sl } => {
                    self.new_epoch();
                    self.new_slot(e, sl);
                }
                Ticks::NEWSLOT { e, sl } => self.new_slot(e, sl),
                Ticks::TOCKS => {
                    info!(target: LOG_T, "tocks");
                    // slot is about to end.
                    // sync, and validate.
                    // no more transactions to be received/send to the end of slot.
                    if self.workspace.is_leader {
                        info!(target: LOG_T, "[leadership won]");
                        //craete block
                        let (block_info, _block_hash) = self.workspace.new_block();
                        //add the block to the blockchain
                        self.add_block(block_info.clone());
                        let block: Block = Block::from(block_info.clone());
                        // publish the block
                        //TODO (fix) before publishing the workspace tx root need to be set.
                        let _ret = self.net.broadcast(block).await;
                    } else {
                        //
                        self.sync_block().await;
                    }
                }
                Ticks::IDLE => continue,
                Ticks::OUTOFSYNC => {
                    error!(target: LOG_T, "clock/blockchain are out of sync");
                    // clock, and blockchain are out of sync
                    let _ = self.clock.sync().await;
                    self.sync_block().await;
                }
            }
            thread::sleep(Duration::from_millis(1000));
            c += 1;
        }
    }

    /// on the onset of the epoch, layout the new the competing coins
    /// assuming static stake during the epoch, enforced by the commitment to competing coins
    /// in the epoch's gen2esis data.
    fn new_epoch(&mut self) {
        info!(target: LOG_T, "[new epoch] {}", self);
        let eta = self.get_eta();
        let mut epoch = Epoch::new(self.epoch_consensus, eta);
        // total stake
        let num_slots = self.workspace.sl;
        let epochs = self.workspace.e;
        let epoch_len = self.epoch_consensus.get_epoch_len();
        // TODO sigma scalar for tunning target function
        // it's value is dependent on the tekonomics,
        // set to one untill then.
        let reward = pallas::Base::one();
        let num_slots = num_slots + epochs * epoch_len;
        let sigma: pallas::Base = pallas::Base::from(num_slots) * reward;
        epoch.create_coins(sigma); // set epoch interal fields working space with competing coins
        self.epoch = epoch.clone();
    }

    /// at the begining of the slot
    /// stakeholder need to play the lottery for the slot.
    /// FIXME if the stakeholder is not winning, staker can try different coins before,
    /// commiting it's coins, to maximize success, thus,
    /// the lottery proof need to be conditioned on the slot itself, and previous proof.
    /// this will encourage each potential leader to play with honesty.
    /// TODO this is fixed by commiting to the stakers at epoch genesis slot
    /// * `e` - epoch index
    /// * `sl` - slot relative index
    fn new_slot(&mut self, e: u64, sl: u64) {
        info!(target: LOG_T, "[new slot] {}, e:{}, rel sl:{}", self, e, sl);
        let st: blake3::Hash = if e > 0 || (e == 0 && sl > 0) {
            self.workspace.block.blockhash()
        } else {
            blake3::hash(b"")
        };
        let is_leader: bool = self.epoch.is_leader(sl);
        // if is leader create proof
        let proof =
            if is_leader { self.epoch.get_proof(sl, &self.pk.clone()) } else { Proof::new(vec![]) };
        // set workspace
        self.workspace.set_sl(sl);
        self.workspace.set_e(e);
        self.workspace.set_st(st);
        self.workspace.set_leader(is_leader);
        self.workspace.set_proof(proof.clone());
        //
        if is_leader {
            let addr = Address::from(self.keypair.public);
            let sign = self.sign(proof.as_ref());
            let stakeholder_meta = StakeholderMetadata::new(sign, addr);
            let ouroboros_meta =
                OuroborosMetadata::new(self.get_eta().to_repr(), TransactionLeadProof::from(proof));
            self.workspace.set_stakeholdermetadata(stakeholder_meta);
            self.workspace.set_ouroborosmetadata(ouroboros_meta);
        }
    }
}

impl fmt::Display for Stakeholder {
    fn fmt(&self, formater: &mut fmt::Formatter) -> fmt::Result {
        formater.write_fmt(format_args!("stakeholder with id: {}", self.id))
    }
}
