/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::{fmt, thread, time::Duration};

use async_std::sync::Arc;
use darkfi_sdk::{
    crypto::{
        constants::MERKLE_DEPTH, schnorr::SchnorrSecret, Address, MerkleNode, PublicKey, SecretKey,
        TokenId,
    },
    incrementalmerkletree::bridgetree::BridgeTree,
    pasta::{group::ff::PrimeField, pallas},
};
use halo2_proofs::arithmetic::Field;
use log::{error, info};
use rand::rngs::OsRng;
use url::Url;

use crate::{
    blockchain::Blockchain,
    consensus::{
        clock::{Clock, Ticks},
        ouroboros::{
            consts::{LOG_T, TREE_LEN},
            Epoch, EpochConsensus, SlotWorkspace, StakeholderState,
        },
        BlockInfo, LeadProof, Metadata,
    },
    crypto::{
        coin::OwnCoin,
        lead_proof,
        leadcoin::LeadCoin,
        proof::{ProvingKey, VerifyingKey},
    },
    zk::{vm::ZkCircuit, vm_stack::{empty_witnesses}},
    zkas::ZkBinary,
    net::{P2p, P2pPtr, Settings, SettingsPtr},
    node::state::state_transition,
    tx::{
        builder::{
            TransactionBuilder, TransactionBuilderClearInputInfo, TransactionBuilderOutputInfo,
        },
        Transaction,
    },
    util::{path::expand_path, time::Timestamp},
    Result,
};

pub struct Stakeholder {
    pub blockchain: Blockchain, // stakeholder view of the blockchain
    pub net: Arc<P2p>,
    pub clock: Clock,
    pub ownedcoins: Vec<OwnCoin>,        // owned stakes
    pub epoch: Epoch,                    // current epoch
    pub epoch_consensus: EpochConsensus, // configuration for the epoch
    pub lead_pk: ProvingKey,
    pub lead_vk: VerifyingKey,
    pub playing: bool,
    pub workspace: SlotWorkspace,
    pub id: i64,
    pub cashier_signature_public: PublicKey,
    pub faucet_signature_public: PublicKey,
    pub cashier_signature_secret: SecretKey,
    pub faucet_signature_secret: SecretKey,
}

impl Stakeholder {
    pub async fn new(
        consensus: EpochConsensus,
        net: P2pPtr,
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
        let bincode = include_bytes!("../../../proof/lead.zk.bin");
        let zkbin = ZkBinary::decode(bincode)?;
        let void_witnesses = empty_witnesses(&zkbin);
        let circuit = ZkCircuit::new(void_witnesses, zkbin);
        let lead_pk = ProvingKey::build(LEADER_PROOF_K, &circuit);
        let lead_vk = VerifyingKey::build(LEADER_PROOF_K, &circuit);
        // let p2p = P2p::new(settings.clone()).await;
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

        info!(target: LOG_T, "stakeholder constructed");
        Ok(Self {
            blockchain: bc,
            net,
            clock,
            ownedcoins: vec![], //TODO should be read from wallet db.
            epoch,
            epoch_consensus: consensus,
            lead_pk,
            lead_vk,
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

    // async fn init_network(&self) -> Result<()> {
    //     info!(target: LOG_T, "init_network()");
    //     let exec = Arc::new(Executor::new());
    //     self.net.clone().start(exec.clone()).await?;
    //     exec.spawn(self.net.clone().run(exec.clone())).detach();
    //     info!(target: LOG_T, "net initialized");
    //     Ok(())
    // }

    pub fn get_net(&self) -> Arc<P2p> {
        info!(target: LOG_T, "get_net()");
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
    // pub async fn sync_block(&self) {
    //     info!(target: LOG_T, "syncing blocks");
    //     for chanptr in self.net.channels().lock().await.values() {
    //         let message_subsytem = chanptr.get_message_subsystem();
    //         message_subsytem.add_dispatch::<BlockInfo>().await;
    //         //TODO start channel if isn't started yet
    //         //let info = chanptr.get_info();
    //         let msg_sub: MessageSubscription<BlockInfo> =
    //             chanptr.subscribe_msg::<BlockInfo>().await.expect("missing blockinfo");

    //         let res = msg_sub.receive().await.unwrap();
    //         let blk: BlockInfo = (*res).to_owned();
    //         //TODO validate the block proof, and transactions.
    //         if self.valid_block(blk.clone()) {
    //             let _len = self.blockchain.add(&[blk]);
    //         } else {
    //             error!(target: LOG_T, "received block is invalid!");
    //         }
    //     }
    // }

    pub async fn background(&mut self, hardlimit: Option<u8>) {
        info!(target: LOG_T, "background");
        // let _ = self.init_network().await;
        let _ = self.clock.sync().await;
        let mut c: u8 = 0;
        let lim: u8 = hardlimit.unwrap_or(0);
        let mut epoch_not_started = true;
        while self.playing {
            if c > lim && lim > 0 {
                break
            }
            // clock ticks slot begins
            // initialize the epoch if it's the time
            // check for leadership
            match self.clock.ticks().await {
                Ticks::GENESIS { e, sl } => {
                    self.new_epoch(e, sl);
                    self.new_slot(e, sl);
                    epoch_not_started = false;
                }
                Ticks::NEWEPOCH { e, sl } => {
                    self.new_epoch(e, sl);
                    //self.new_slot(e, sl);
                    epoch_not_started = false;
                }
                Ticks::NEWSLOT { e, sl } => {
                    if !epoch_not_started {
                        self.new_slot(e, sl);
                    }
                }
                Ticks::TOCKS => {
                    info!(target: LOG_T, "tocks");
                    // slot is about to end.
                    // sync, and validate.
                    // no more transactions to be received/send to the end of slot.
                    if self.workspace.has_leader() {
                        info!(target: LOG_T, "[leadership won]");
                        //craete block
                        let (block_info, _block_hash) = self.workspace.new_block();
                        //add the block to the blockchain
                        self.add_block(block_info.clone());
                        // let block: Block = Block::from(block_info.clone());
                        // publish the block
                        //TODO (fix) before publishing the workspace tx root need to be set.
                        self.net.broadcast(block_info.clone()).await.unwrap();
                    }
                }
                Ticks::IDLE => continue,
                Ticks::OUTOFSYNC => {
                    error!(target: LOG_T, "clock/blockchain are out of sync");
                    // clock, and blockchain are out of sync
                    let _ = self.clock.sync().await;
                    // self.sync_block().await;
                }
            }
            thread::sleep(Duration::from_millis(1000));
            c += 1;
        }
    }

    /// on the onset of the epoch, layout the new the competing coins
    /// assuming static stake during the epoch, enforced by the commitment to competing coins
    /// in the epoch's gen2esis data.
    fn new_epoch(&mut self, e: u64, sl: u64) {
        info!(target: LOG_T, "[new epoch] {}", self);
        let eta = self.get_eta();
        let mut epoch = Epoch::new(self.epoch_consensus, eta);
        epoch.create_coins(e, sl, &self.ownedcoins);
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

        let (won, idx) = self.epoch.is_leader(sl);
        info!("Lottery outcome: {}", won);
        if !won {
            return
        }
        // TODO: Generate rewards transaction
        info!("Winning coin index: {}", idx);
        // Generating leader proof
        let coin = self.epoch.get_coin(sl as usize, idx);
        // TODO: Generate new LeadCoin from newlly minted coin, will reuse original coin for now
        //let coin2 = something();
        let proof = self.epoch.get_proof(sl as usize, idx, &self.get_leadprovkingkey());
        //Verifying generated proof against winning coin public inputs
        info!("Leader proof generated successfully, veryfing...");
        match lead_proof::verify_lead_proof(
            &self.get_leadverifyingkey(),
            &proof,
            &coin.public_inputs(),
        ) {
            Ok(_) => info!("Proof veryfied succsessfully!"),
            Err(e) => error!("Error during leader proof verification: {}", e),
        }

        self.workspace.add_leader(won);
        self.workspace.set_idx(idx);
        let keypair = coin.keypair.unwrap();
        let addr = Address::from(keypair.public);
        let sign = keypair.secret.sign(&mut OsRng, proof.as_ref());
        let meta = Metadata::new(
            sign,
            addr,
            coin.public_inputs(),
            coin.public_inputs(),
            idx,
            coin.sn.unwrap(),
            self.get_eta().to_repr(),
            LeadProof::from(proof),
            vec![],
        );
        self.workspace.add_metadata(meta);
        let owned_coin = self.finalize_coin(&self.epoch.get_coin(sl as usize, idx as usize));
        self.ownedcoins.push(owned_coin);
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

        let token_id = TokenId::from(pallas::Base::random(&mut OsRng));
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
