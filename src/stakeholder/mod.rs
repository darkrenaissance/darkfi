use smol::Executor;
use async_std::sync::Arc;
use halo2_proofs::arithmetic::Field;
use log::{debug, error, info};
use std::fmt;

use rand::rngs::OsRng;
use std::{thread, time::Duration};

use crate::zk::circuit::{BurnContract, LeadContract, MintContract};
use incrementalmerkletree::{bridgetree::BridgeTree, Tree};

pub mod types;
pub mod consts;
pub mod utils;

use crate::{
    blockchain::{Blockchain},
    consensus::{
        clock::{Clock, Ticks},
        Block, BlockInfo, Header, Metadata,
        LeadProof,
    },
    crypto::{
        address::Address,
        coin::OwnCoin,
        constants::MERKLE_DEPTH,
        keypair::{PublicKey, SecretKey},
        util::poseidon_hash,
        leadcoin::LeadCoin,
        merkle_node::MerkleNode,
        note::{EncryptedNote, Note},
        nullifier::Nullifier,
        proof::{Proof, ProvingKey, VerifyingKey},
        schnorr::{SchnorrSecret},
    },
    net::{MessageSubscription, P2p, Settings, SettingsPtr},
    node::state::{state_transition, ProgramState, StateUpdate},
    tx::{
        builder::{
            TransactionBuilder, TransactionBuilderClearInputInfo, TransactionBuilderOutputInfo,
        },
        Transaction,
    },
    util::{path::expand_path, time::Timestamp},
    stakeholder::types::{Float10},
    stakeholder::consts::{RADIX_BITS, LOG_T, TREE_LEN, P},
    stakeholder::utils::{fbig2base},
    Result,
};

use url::Url;

use pasta_curves::pallas;

use group::ff::PrimeField;

pub mod epoch;
pub use epoch::{Epoch, EpochConsensus};

pub(crate) mod workspace;
pub(crate) use workspace::SlotWorkspace;


struct StakeholderState {
    /// The entire Merkle tree state
    tree: BridgeTree<MerkleNode, MERKLE_DEPTH>,
    /// List of all previous and the current Merkle roots.
    /// This is the hashed value of all the children.
    merkle_roots: Vec<MerkleNode>,
    /// Nullifiers prevent double spending
    nullifiers: Vec<Nullifier>,
    /// All received coins
    // NOTE: We need maybe a flag to keep track of which ones are
    // spent. Maybe the spend field links to a tx hash:input index.
    // We should also keep track of the tx hash:output index where
    // this coin was received.
    own_coins: Vec<OwnCoin>,
    /// Verifying key for the mint zk circuit.
    mint_vk: VerifyingKey,
    /// Verifying key for the burn zk circuit.
    burn_vk: VerifyingKey,

    /// Public key of the cashier
    cashier_signature_public: PublicKey,

    /// Public key of the faucet
    faucet_signature_public: PublicKey,

    /// List of all our secret keys
    secrets: Vec<SecretKey>,
}

impl ProgramState for StakeholderState {
    fn is_valid_cashier_public_key(&self, public: &PublicKey) -> bool {
        public == &self.cashier_signature_public
    }

    fn is_valid_faucet_public_key(&self, public: &PublicKey) -> bool {
        public == &self.faucet_signature_public
    }

    fn is_valid_merkle(&self, merkle_root: &MerkleNode) -> bool {
        self.merkle_roots.iter().any(|m| m == merkle_root)
    }

    fn nullifier_exists(&self, nullifier: &Nullifier) -> bool {
        self.nullifiers.iter().any(|n| n == nullifier)
    }

    fn mint_vk(&self) -> &VerifyingKey {
        &self.mint_vk
    }

    fn burn_vk(&self) -> &VerifyingKey {
        &self.burn_vk
    }
}

impl StakeholderState {
    fn apply(&mut self, mut update: StateUpdate) {
        // Extend our list of nullifiers with the ones from the update
        self.nullifiers.append(&mut update.nullifiers);

        // Update merkle tree and witnesses
        for (coin, enc_note) in update.coins.into_iter().zip(update.enc_notes.into_iter()) {
            // Add the new coins to the Merkle tree
            let node = MerkleNode(coin.0);
            self.tree.append(&node);

            // Keep track of all Merkle roots that have existed
            self.merkle_roots.push(self.tree.root(0).unwrap());

            // If it's our own coin, witness it and append to the vector.
            if let Some((note, secret)) = self.try_decrypt_note(enc_note) {
                let leaf_position = self.tree.witness().unwrap();
                let nullifier = poseidon_hash::<2>([secret.inner(), note.serial]);
                let own_coin = OwnCoin {
                    coin: coin,
                    note: note,
                    secret: secret,
                    nullifier: Nullifier::from(nullifier),
                    leaf_position: leaf_position
                };
                self.own_coins.push(own_coin);
            }
        }
    }

    fn try_decrypt_note(&self, ciphertext: EncryptedNote) -> Option<(Note, SecretKey)> {
        // Loop through all our secret keys...
        for secret in &self.secrets {
            // .. attempt to decrypt the note ...
            if let Ok(note) = ciphertext.decrypt(secret) {
                // ... and return the decrypted note for this coin.
                return Some((note, *secret))
            }
        }

        // We weren't able to decrypt the note with any of our keys.
        None
    }
}

pub struct Stakeholder {
    pub blockchain: Blockchain, // stakeholder view of the blockchain
    pub net: Arc<P2p>,
    pub clock: Clock,
    pub ownedcoins: Vec<OwnCoin>,        // owned stakes
    pub epoch: Epoch,                    // current epoch
    pub epoch_consensus: EpochConsensus, // configuration for the epoch
    pub lead_pk: ProvingKey,
    pub mint_pk: ProvingKey,
    pub burn_pk: ProvingKey,
    pub lead_vk: VerifyingKey,
    pub mint_vk: VerifyingKey,
    pub burn_vk: VerifyingKey,
    pub playing: bool,
    pub workspace: SlotWorkspace,
    pub id: i64,
    pub cashier_signature_public: PublicKey,
    pub faucet_signature_public: PublicKey,
    pub cashier_signature_secret: SecretKey,
    pub faucet_signature_secret: SecretKey,
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
        let bc = Blockchain::new(&db, ts, genesis_hash).unwrap();
        let eta = pallas::Base::one();
        let epoch = Epoch::new(consensus, eta);

        let lead_pk = ProvingKey::build(k.unwrap(), &LeadContract::default());
        let mint_pk = ProvingKey::build(k.unwrap(), &MintContract::default());
        let burn_pk = ProvingKey::build(k.unwrap(), &BurnContract::default());
        let lead_vk = VerifyingKey::build(k.unwrap(), &LeadContract::default());
        let mint_vk = VerifyingKey::build(k.unwrap(), &MintContract::default());
        let burn_vk = VerifyingKey::build(k.unwrap(), &BurnContract::default());
        let p2p = P2p::new(settings.clone()).await;
        let workspace = SlotWorkspace::default();
        let clock = Clock::new(
            Some(consensus.get_epoch_len()),
            Some(consensus.get_slot_len()),
            Some(consensus.get_tick_len()),
            settings.peers,
        );
        let cashier_signature_secret = SecretKey::random(&mut OsRng);
        let cashier_signature_public = PublicKey::from_secret(cashier_signature_secret);

        let faucet_signature_secret = SecretKey::random(&mut OsRng);
        let faucet_signature_public = PublicKey::from_secret(faucet_signature_secret);

        debug!(target: LOG_T, "stakeholder constructed");
        Ok(Self {
            blockchain: bc,
            net: p2p,
            clock,
            ownedcoins: vec![], //TODO should be read from wallet db.
            epoch,
            epoch_consensus: consensus,
            lead_pk,
            mint_pk,
            burn_pk,
            lead_vk,
            mint_vk,
            burn_vk,
            playing: true,
            workspace,
            id,
            cashier_signature_public,
            faucet_signature_public,
            cashier_signature_secret,
            faucet_signature_secret,
        })
    }

    /*
    /// wrapper on schnorr public verify
    pub fn verify(&self, message: &[u8], signature: &Signature) -> bool {
        info!(target: LOG_T, "verify()");
        self.keypair.public.verify(message, signature)
    }
    */

    pub fn get_leadprovkingkey(&self) -> ProvingKey {
        info!(target: LOG_T, "get_leadprovkingkey()");
        self.lead_pk.clone()
    }

    pub fn get_mintprovkingkey(&self) -> ProvingKey {
        info!(target: LOG_T, "get_mintprovkingkey()");
        self.mint_pk.clone()
    }

    pub fn get_burnprovkingkey(&self) -> ProvingKey {
        info!(target: LOG_T, "get_burnprovkingkey()");
        self.burn_pk.clone()
    }

    pub fn get_leadverifyingkey(&self) -> VerifyingKey {
        info!(target: LOG_T, "get_leadverifyingkey()");
        self.lead_vk.clone()
    }

    pub fn get_mintverifyingkey(&self) -> VerifyingKey {
        info!(target: LOG_T, "get_mintverifyingkey()");
        self.mint_vk.clone()
    }

    pub fn get_burnverifyingkey(&self) -> VerifyingKey {
        info!(target: LOG_T, "get_burnverifyingkey()");
        self.burn_vk.clone()
    }

    /// get list stakeholder peers on the p2p network for synchronization
    pub fn get_peers(&self) -> Vec<Url> {
        info!(target: LOG_T, "get_peers()");
        let settings: SettingsPtr = self.net.settings();
        settings.peers.clone()
    }


    async fn init_network(&self) -> Result<()> {
        info!(target: LOG_T, "init_network()");
        let exec = Arc::new(Executor::new());
        self.net.clone().start(exec.clone()).await?;
        exec.spawn(self.net.clone().run(exec.clone())).detach();
        info!(target: LOG_T, "net initialized");
        Ok(())
    }

    pub fn get_net(&self) -> Arc<P2p> {
        info!(target: LOG_T, "get_net()");
        //TODO use P2p ptr not to overwrite wrappers
        self.net.clone()
    }

    /// add new blockinfo to the blockchain
    pub fn add_block(&self, block: BlockInfo) {
        info!(target: LOG_T, "add_block()");
        let blocks = [block];
        let _len = self.blockchain.add(&blocks);
    }

    pub fn add_tx(&mut self, tx: Transaction) {
        info!(target: LOG_T, "add_tx()");
        self.workspace.add_tx(tx);
    }

    /// extract leader selection lottery randomness \eta
    /// it's the hash of the previous lead proof
    /// converted to pallas base
    pub fn get_eta(&self) -> pallas::Base {
        info!(target: LOG_T, "get_eta()");

        let proof_tx_hash = self.blockchain.get_last_proof_hash().unwrap();
        let mut bytes: [u8; 32] = *proof_tx_hash.as_bytes();
        // read first 254 bits
        bytes[30] = 0;
        bytes[31] = 0;
        pallas::Base::from_repr(bytes).unwrap()
    }

    pub fn valid_block(&self, _blk: BlockInfo) -> bool {
        info!(target: LOG_T, "valid_block()");

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
        info!(target: LOG_T, "background");
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
                    self.new_epoch(e, sl);
                    self.new_slot(e, sl);
                }
                Ticks::NEWEPOCH { e, sl } => {
                    self.new_epoch(e, sl);
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

    fn get_f(&self) -> Float10 {
        //TODO (res) should be function of the average time to end of slot
        // in the previous epoch.
        let one : Float10 = Float10::from_str_native("1")
            .unwrap()
            .with_precision(RADIX_BITS)
            .value();
        let two : Float10 = Float10::from_str_native("2")
            .unwrap()
            .with_precision(RADIX_BITS)
            .value();
        one/two
    }
    /// on the onset of the epoch, layout the new the competing coins
    /// assuming static stake during the epoch, enforced by the commitment to competing coins
    /// in the epoch's gen2esis data.
    fn new_epoch(&mut self, e: u64, sl: u64) {
        info!(target: LOG_T, "[new epoch] {}", self);
        let eta = self.get_eta();
        let mut epoch = Epoch::new(self.epoch_consensus, eta);
        // total stake
        // let rel_sl = self.workspace.sl;
        // let epochs = self.workspace.e;
        // let epoch_len = self.epoch_consensus.get_epoch_len();
        // let abs_sl = rel_sl + epochs * epoch_len;
        //
        let f = self.get_f();
        let total_stake = self.epoch.consensus.total_stake(e, sl);
        let one : Float10 = Float10::from_str_native("1")
            .unwrap()
            .with_precision(RADIX_BITS)
            .value();
        let two : Float10 = Float10::from_str_native("2")
            .unwrap()
            .with_precision(RADIX_BITS)
            .value();
        //TODO should set f precision here
        /*
        let f : Float10 =  Float10::try_from(f_val)
            .unwrap()
            .with_precision(RADIX_BITS)
        .value();
        */
        let field_p = Float10::from_str_native(P)
            .unwrap()
            .with_precision(RADIX_BITS)
            .value();
        let total_sigma  = Float10::try_from(total_stake)
            .unwrap()
            .with_precision(RADIX_BITS)
            .value();
        let x = one - f;
        info!("x: {}", x);
        // also ln small x should work normally.
        let c = x.ln();
        info!("c: {}", c);
        let sigma1_fbig = c.clone()/total_sigma.clone() * field_p.clone();
        info!("sigma1: {}", sigma1_fbig);

        //TODO in sigma calculation get rad if exp is neg
        let sigma1 : pallas::Base = fbig2base(sigma1_fbig);
        info!("sigma1 base: {:?}", sigma1);
        let sigma2_fbig = c.clone()/total_sigma.clone() * c.clone()/total_sigma.clone()  * field_p.clone()/two.clone();
        info!("sigma2: {}", sigma2_fbig);
        let sigma2 : pallas::Base = fbig2base(sigma2_fbig);
        info!("sigma2 base: {:?}", sigma2);
        epoch.create_coins(sigma1, sigma2, self.ownedcoins.clone()); // set epoch interal fields working space with competing coins
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
        // set workspace
        self.workspace.set_sl(sl);
        self.workspace.set_e(e);
        self.workspace.set_st(st);
        let mut winning_coin_idx: usize = 0;
        let won = self.epoch.is_leader(sl, &mut winning_coin_idx);
        let proof = if won {
            self.epoch.get_proof(sl, winning_coin_idx, &self.get_leadprovkingkey())
        } else {
            Proof::new(vec![])
        };
        self.workspace.set_leader(won);
        self.workspace.set_proof(proof.clone());

        let coin = self.epoch.get_coin(sl as usize, winning_coin_idx as usize);
        let keypair = coin.keypair.unwrap();
        let addr = Address::from(keypair.public);
        let sign = keypair.secret.sign(proof.as_ref());
        let meta =
            Metadata::new(sign, addr, self.get_eta().to_repr(), LeadProof::from(proof), vec![]);
        self.workspace.set_metadata(meta);
        //
        if won {
            //TODO (res) verify the coin is finalized
            // could be finalized in later slot accord to the finalization policy that is WIP.
            let owned_coin =
                self.finalize_coin(&self.epoch.get_coin(sl as usize, winning_coin_idx as usize));
            self.ownedcoins.push(owned_coin);
        }
    }

    //TODO (res) validate the owncoin is the same winning leadcoin
    pub fn finalize_coin(&self, coin: &LeadCoin) -> OwnCoin {
        info!(target: LOG_T, "finalize coin");
        let keypair = coin.keypair.unwrap();
        let mut state = StakeholderState {
            tree: BridgeTree::<MerkleNode, MERKLE_DEPTH>::new(TREE_LEN),
            merkle_roots: vec![],
            nullifiers: vec![],
            own_coins: vec![],
            mint_vk: self.mint_vk.clone(),
            burn_vk: self.burn_vk.clone(),
            cashier_signature_public: self.cashier_signature_public,
            faucet_signature_public: self.faucet_signature_public,
            secrets: vec![keypair.secret],
        };

        let token_id = pallas::Base::random(&mut OsRng);
        let builder = TransactionBuilder {
            clear_inputs: vec![TransactionBuilderClearInputInfo {
                value: coin.value.unwrap(),
                token_id,
                signature_secret: self.cashier_signature_secret,
            }],
            inputs: vec![],
            outputs: vec![TransactionBuilderOutputInfo {
                value: coin.value.unwrap(),
                token_id,
                public: keypair.public,
            }],
        };
        let tx = builder.build(&self.mint_pk, &self.burn_pk).unwrap();

        tx.verify(&state.mint_vk, &state.burn_vk).unwrap();
        let _note = tx.outputs[0].enc_note.decrypt(&keypair.secret).unwrap();
        let update = state_transition(&state, tx).unwrap();
        state.apply(update);
        state.own_coins[0].clone()
    }
}

impl fmt::Display for Stakeholder {
    fn fmt(&self, formater: &mut fmt::Formatter) -> fmt::Result {
        formater.write_fmt(format_args!("stakeholder with id: {}", self.id))
    }
}
