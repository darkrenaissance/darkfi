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

use std::{collections::HashMap, io::Cursor, time::Duration};

use async_std::sync::{Arc, RwLock};
use chrono::{NaiveDateTime, Utc};
use darkfi_sdk::{
    crypto::{
        constants::MERKLE_DEPTH,
        schnorr::{SchnorrPublic, SchnorrSecret},
        ContractId, MerkleNode, PublicKey,
    },
    db::ZKAS_DB_NAME,
};
use darkfi_serial::{
    deserialize, serialize, Decodable, Encodable, SerialDecodable, SerialEncodable, WriteExt,
};
use incrementalmerkletree::{bridgetree::BridgeTree, Tree};
use log::{debug, error, info, warn};
use pasta_curves::{group::ff::PrimeField, pallas};
use rand::{rngs::OsRng, thread_rng, Rng};
use serde_json::json;

use super::{
    constants,
    leadcoin::{LeadCoin, LeadCoinSecrets},
    utils::fbig2base,
    Block, BlockInfo, BlockProposal, Float10, Header, LeadInfo, LeadProof, ProposalChain,
};

use crate::{
    blockchain::Blockchain,
    crypto::proof::{ProvingKey, VerifyingKey},
    net,
    rpc::jsonrpc::JsonNotification,
    runtime::vm_runtime::Runtime,
    system::{Subscriber, SubscriberPtr},
    tx::Transaction,
    util::time::Timestamp,
    wallet::WalletPtr,
    zk::{vm::ZkCircuit, vm_stack::empty_witnesses},
    zkas::ZkBinary,
    Error, Result,
};

/// This struct represents the information required by the consensus algorithm
#[derive(Debug)]
pub struct ConsensusState {
    /// Genesis block creation timestamp
    pub genesis_ts: Timestamp,
    /// Genesis block hash
    pub genesis_block: blake3::Hash,
    /// Participating start slot
    pub participating: Option<u64>,
    /// Last slot node check for finalization
    pub checked_finalization: u64,
    /// Slots offset since genesis,
    pub offset: Option<u64>,
    /// Fork chains containing block proposals
    pub proposals: Vec<ProposalChain>,
    /// Current epoch
    pub epoch: u64,
    /// Current epoch eta
    pub epoch_eta: pallas::Base,
    /// Current epoch competing coins
    pub coins: Vec<Vec<LeadCoin>>,
    // TODO: Aren't these already in db after finalization?
    /// Seen nullifiers from proposals
    pub leaders_nullifiers: Vec<pallas::Base>,
    /// Seen spent coins from proposals
    pub leaders_spent_coins: Vec<(pallas::Base, pallas::Base)>,
    /// Leaders count history
    pub leaders_history: Vec<u64>,
    /// Kp
    pub kp: Float10,
}

impl ConsensusState {
    pub fn new(genesis_ts: Timestamp, genesis_data: blake3::Hash) -> Result<Self> {
        let genesis_block = Block::genesis_block(genesis_ts, genesis_data).blockhash();

        Ok(Self {
            genesis_ts,
            genesis_block,
            participating: None,
            checked_finalization: 0,
            offset: None,
            proposals: vec![],
            epoch: 0,
            epoch_eta: pallas::Base::one(),
            coins: vec![],
            leaders_nullifiers: vec![],
            leaders_spent_coins: vec![],
            leaders_history: vec![0],
            kp: constants::FLOAT10_THREE.clone() / constants::FLOAT10_NINE.clone(),
        })
    }
}

/// Auxiliary structure used for consensus syncing.
#[derive(Debug, SerialEncodable, SerialDecodable)]
pub struct ConsensusRequest {}

impl net::Message for ConsensusRequest {
    fn name() -> &'static str {
        "consensusrequest"
    }
}

/// Auxiliary structure used for consensus syncing.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ConsensusResponse {
    /// Slots offset since genesis,
    pub offset: Option<u64>,
    /// Hot/live data used by the consensus algorithm
    pub proposals: Vec<ProposalChain>,
    /// Pending transactions
    pub unconfirmed_txs: Vec<Transaction>,
    /// Seen nullifiers from proposals
    pub leaders_nullifiers: Vec<pallas::Base>,
    /// Seen spent coins from proposals
    pub leaders_spent_coins: Vec<(pallas::Base, pallas::Base)>,
}

impl net::Message for ConsensusResponse {
    fn name() -> &'static str {
        "consensusresponse"
    }
}

/// Atomic pointer to validator state.
pub type ValidatorStatePtr = Arc<RwLock<ValidatorState>>;

/// This struct represents the state of a validator node.
pub struct ValidatorState {
    /// Leader proof proving key
    pub lead_proving_key: Option<ProvingKey>,
    /// Leader proof verifying key
    pub lead_verifying_key: VerifyingKey,
    /// Hot/Live data used by the consensus algorithm
    pub consensus: ConsensusState,
    /// Canonical (finalized) blockchain
    pub blockchain: Blockchain,
    /// Pending transactions
    pub unconfirmed_txs: Vec<Transaction>,
    /// A map of various subscribers exporting live info from the blockchain
    /// TODO: Instead of JsonNotification, it can be an enum of internal objects,
    ///       and then we don't have to deal with json in this module but only
    //        externally.
    pub subscribers: HashMap<&'static str, SubscriberPtr<JsonNotification>>,
    /// ZK proof verifying keys for smart contract calls
    pub verifying_keys: Arc<RwLock<HashMap<[u8; 32], Vec<(String, VerifyingKey)>>>>,
    /// Wallet interface
    pub wallet: WalletPtr,
}

impl ValidatorState {
    pub async fn new(
        db: &sled::Db, // <-- TODO: Avoid this with some wrapping, sled should only be in blockchain
        genesis_ts: Timestamp,
        genesis_data: blake3::Hash,
        wallet: WalletPtr,
        faucet_pubkeys: Vec<PublicKey>,
        enable_participation: bool,
    ) -> Result<ValidatorStatePtr> {
        info!("Initializing ValidatorState");

        info!("Initializing wallet tables for consensus");
        // TODO: TESTNET: The stuff is kept entirely in memory for now, what should we write
        //                to disk/wallet?
        //let consensus_tree_init_query = include_str!("../../script/sql/consensus_tree.sql");
        //let consensus_keys_init_query = include_str!("../../script/sql/consensus_keys.sql");
        //wallet.exec_sql(consensus_tree_init_query).await?;
        //wallet.exec_sql(consensus_keys_init_query).await?;

        info!("Generating leader proof keys with k: {}", constants::LEADER_PROOF_K);
        let bincode = include_bytes!("../../proof/lead.zk.bin");
        let zkbin = ZkBinary::decode(bincode)?;
        let witnesses = empty_witnesses(&zkbin);
        let circuit = ZkCircuit::new(witnesses, zkbin);

        let lead_verifying_key = VerifyingKey::build(constants::LEADER_PROOF_K, &circuit);
        // We only need this proving key if we're going to participate in the consensus.
        let lead_proving_key = if enable_participation {
            Some(ProvingKey::build(constants::LEADER_PROOF_K, &circuit))
        } else {
            None
        };

        let consensus = ConsensusState::new(genesis_ts, genesis_data)?;
        let blockchain = Blockchain::new(db, genesis_ts, genesis_data)?;

        let unconfirmed_txs = vec![];

        // -----NATIVE WASM CONTRACTS-----
        // This is the current place where native contracts are being deployed.
        // When the `Blockchain` object is created, it doesn't care whether it
        // already has the contract data or not. If there's existing data, it
        // will just open the necessary db and trees, and give back what it has.
        // This means that on subsequent runs our native contracts will already
        // be in a deployed state, so what we actually do here is a redeployment.
        // This kind of operation should only modify the contract's state in case
        // it wasn't deployed before (meaning the initial run). Otherwise, it
        // shouldn't touch anything, or just potentially update the db schemas or
        // whatever is necessary. This logic should be handled in the init function
        // of the actual contract, so make sure the native contracts handle this well.

        // FIXME: This ID should be something that does not solve the pallas curve equation,
        // and/or just hardcoded and forbidden in non-native contract deployment.
        let money_contract_id = ContractId::from(pallas::Base::from(u64::MAX - 420));
        // The faucet pubkeys are pubkeys which are allowed to create clear inputs
        // in the money contract.
        let money_contract_deploy_payload = serialize(&faucet_pubkeys);

        // In this hashmap, we keep references to ZK proof verifying keys needed
        // for the circuits our native contracts provide.
        let mut verifying_keys = HashMap::new();

        let native_contracts = vec![(
            "Money Contract",
            money_contract_id,
            include_bytes!("../contract/money/money_contract.wasm"),
            money_contract_deploy_payload,
        )];

        info!("Deploying native wasm contracts");
        for nc in native_contracts {
            info!("Deploying {} with ContractID {}", nc.0, nc.1);
            let mut runtime = Runtime::new(&nc.2[..], blockchain.clone(), nc.1)?;
            runtime.deploy(&nc.3)?;
            info!("Successfully deployed {}", nc.0);

            // When deployed, we can do a lookup for the zkas circuits and
            // initialize verifying keys for them.
            info!("Creating ZK verifying keys for {} zkas circuits", nc.0);
            debug!("Looking up zkas db for {} (ContractID: {})", nc.0, nc.1);
            let zkas_db = blockchain.contracts.lookup(&blockchain.sled_db, &nc.1, ZKAS_DB_NAME)?;

            let mut vks = vec![];
            for i in zkas_db.iter() {
                debug!("Iterating over zkas db");
                let (zkas_ns, zkas_bincode) = i?;
                debug!("Deserializing namespace");
                let zkas_ns: String = deserialize(&zkas_ns)?;
                info!("Creating VerifyingKey for zkas circuit with namespace {}", zkas_ns);
                let zkbin = ZkBinary::decode(&zkas_bincode)?;
                let circuit = ZkCircuit::new(empty_witnesses(&zkbin), zkbin);
                // FIXME: This k=13 man...
                let vk = VerifyingKey::build(13, &circuit);
                vks.push((zkas_ns, vk));
            }

            info!("Finished creating VerifyingKey objects for {} (ContractID: {})", nc.0, nc.1);
            verifying_keys.insert(nc.1.to_bytes(), vks);
        }
        info!("Finished deployment of native wasm contracts");
        // -----NATIVE WASM CONTRACTS-----

        // Here we initialize various subscribers that can export live consensus/blockchain data.
        let mut subscribers = HashMap::new();
        let block_subscriber = Subscriber::new();
        subscribers.insert("blocks", block_subscriber);

        let state = Arc::new(RwLock::new(ValidatorState {
            lead_proving_key,
            lead_verifying_key,
            consensus,
            blockchain,
            unconfirmed_txs,
            subscribers,
            verifying_keys: Arc::new(RwLock::new(verifying_keys)),
            wallet,
        }));

        Ok(state)
    }

    /// The node retrieves a transaction, validates its state transition,
    /// and appends it to the unconfirmed transactions list.
    pub async fn append_tx(&mut self, tx: Transaction) -> bool {
        let tx_hash = blake3::hash(&serialize(&tx));
        let tx_in_txstore = match self.blockchain.transactions.contains(&tx_hash) {
            Ok(v) => v,
            Err(e) => {
                error!("append_tx(): Failed querying txstore: {}", e);
                return false
            }
        };

        if self.unconfirmed_txs.contains(&tx) || tx_in_txstore {
            debug!("append_tx(): We have already seen this tx.");
            return false
        }

        debug!("append_tx(): Starting state transition validation");
        if let Err(e) = self.verify_transactions(&[tx.clone()], false).await {
            error!("append_tx(): Failed to verify transaction: {}", e);
            return false
        };

        debug!("append_tx(): Appended tx to mempool");
        self.unconfirmed_txs.push(tx);
        true
    }

    /// Calculates current epoch.
    pub fn current_epoch(&self) -> u64 {
        self.slot_epoch(self.current_slot())
    }

    /// Calculates the epoch of the provided slot.
    /// Epoch duration is configured using the `EPOCH_LENGTH` value.
    pub fn slot_epoch(&self, slot: u64) -> u64 {
        slot / constants::EPOCH_LENGTH as u64
    }

    /// Calculates current slot, based on elapsed time from the genesis block.
    /// Slot duration is configured using the `SLOT_TIME` constant.
    pub fn current_slot(&self) -> u64 {
        self.consensus.genesis_ts.elapsed() / constants::SLOT_TIME
    }

    /// Calculates the relative number of the provided slot.
    pub fn relative_slot(&self, slot: u64) -> u64 {
        slot % constants::EPOCH_LENGTH as u64
    }

    /// Finds the last slot a proposal or block was generated.
    pub fn last_slot(&self) -> Result<u64> {
        let mut slot = 0;
        for chain in &self.consensus.proposals {
            for proposal in &chain.proposals {
                if proposal.block.header.slot > slot {
                    slot = proposal.block.header.slot;
                }
            }
        }

        // We return here in case proposals exist,
        // so we don't query the sled database.
        if slot > 0 {
            return Ok(slot)
        }

        let (last_slot, _) = self.blockchain.last()?;
        Ok(last_slot)
    }

    /// Calculates seconds until next Nth slot starting time.
    /// Slots duration is configured using the SLOT_TIME constant.
    pub fn next_n_slot_start(&self, n: u64) -> Duration {
        assert!(n > 0);
        let start_time = NaiveDateTime::from_timestamp(self.consensus.genesis_ts.0, 0);
        let current_slot = self.current_slot() + n;
        let next_slot_start =
            (current_slot * constants::SLOT_TIME) + (start_time.timestamp() as u64);
        let next_slot_start = NaiveDateTime::from_timestamp(next_slot_start as i64, 0);
        let current_time = NaiveDateTime::from_timestamp(Utc::now().timestamp(), 0);
        let diff = next_slot_start - current_time;

        Duration::new(diff.num_seconds().try_into().unwrap(), 0)
    }

    /// Calculate slots until next Nth epoch.
    /// Epoch duration is configured using the EPOCH_LENGTH value.
    pub fn slots_to_next_n_epoch(&self, n: u64) -> u64 {
        assert!(n > 0);
        let slots_till_next_epoch =
            constants::EPOCH_LENGTH as u64 - self.relative_slot(self.current_slot());
        ((n - 1) * constants::EPOCH_LENGTH as u64) + slots_till_next_epoch
    }

    /// Calculates seconds until next Nth epoch starting time.
    pub fn next_n_epoch_start(&self, n: u64) -> Duration {
        self.next_n_slot_start(self.slots_to_next_n_epoch(n))
    }

    /// Set participating slot to next.
    pub fn set_participating(&mut self) -> Result<()> {
        self.consensus.participating = Some(self.current_slot() + 1);
        Ok(())
    }

    /// Check if new epoch has started, to create new epoch coins.
    /// Returns flag to signify if epoch has changed and vector of
    /// new epoch competing coins.
    pub async fn epoch_changed(&mut self) -> Result<bool> {
        let epoch = self.current_epoch();
        if epoch <= self.consensus.epoch {
            return Ok(false)
        }
        let eta = self.get_eta();
        // TODO: slot parameter should be absolute slot, not relative.
        // At start of epoch, relative slot is 0.
        self.consensus.coins = self.create_epoch_coins(eta, epoch).await?;
        self.consensus.epoch = epoch;
        self.consensus.epoch_eta = eta;
        Ok(true)
    }

    /// return 2-term target approximation sigma coefficients.
    /// `epoch: absolute epoch index
    /// `slot: relative slot index
    fn sigmas(&mut self, epoch: u64, slot: u64) -> (pallas::Base, pallas::Base) {
        let f = self.win_prob_with_full_stake();

        // Generate sigmas
        let total_stake = self.total_stake_plus(epoch, slot); // Only used for fine-tuning

        let one = constants::FLOAT10_ONE.clone();
        let two = constants::FLOAT10_TWO.clone();
        let field_p = Float10::from_str_native(constants::P)
            .unwrap()
            .with_precision(constants::RADIX_BITS)
            .value();
        let total_sigma =
            Float10::try_from(total_stake).unwrap().with_precision(constants::RADIX_BITS).value();

        let x = one - f;
        let c = x.ln();

        let sigma1_fbig = c.clone() / total_sigma.clone() * field_p.clone();
        let sigma1 = fbig2base(sigma1_fbig);

        let sigma2_fbig = (c / total_sigma).powf(two.clone()) * (field_p / two);
        let sigma2 = fbig2base(sigma2_fbig);
        (sigma1, sigma2)
    }

    /// Generate epoch-competing coins
    async fn create_epoch_coins(
        &self,
        eta: pallas::Base,
        epoch: u64,
    ) -> Result<Vec<Vec<LeadCoin>>> {
        info!("Consensus: Creating coins for epoch: {}", epoch);
        self.create_coins(eta).await
    }

    /// Generate coins for provided sigmas.
    /// NOTE: The strategy here is having a single competing coin per slot.
    async fn create_coins(&self, eta: pallas::Base) -> Result<Vec<Vec<LeadCoin>>> {
        let slot = self.current_slot();
        let mut rng = thread_rng();

        let mut seeds: Vec<u64> = Vec::with_capacity(constants::EPOCH_LENGTH);
        for _ in 0..constants::EPOCH_LENGTH {
            seeds.push(rng.gen());
        }

        let epoch_secrets = LeadCoinSecrets::generate();

        let mut tree_cm = BridgeTree::<MerkleNode, MERKLE_DEPTH>::new(constants::EPOCH_LENGTH);
        // LeadCoin matrix where each row represents a slot and contains its competing coins.
        let mut coins: Vec<Vec<LeadCoin>> = Vec::with_capacity(constants::EPOCH_LENGTH);

        // TODO: TESTNET: Here we would look into the wallet to find coins we're able to use.
        //                The wallet has specific tables for consensus coins.
        // TODO: TESTNET: Token ID still has to be enforced properly in the consensus.

        // Temporarily, we compete with zero stake
        for i in 0..constants::EPOCH_LENGTH {
            let coin = LeadCoin::new(
                eta,
                constants::LOTTERY_HEAD_START, // TODO: TESTNET: Why is this constant being used?
                slot + i as u64,
                epoch_secrets.secret_keys[i].inner(),
                epoch_secrets.merkle_roots[i],
                i,
                epoch_secrets.merkle_paths[i],
                seeds[i],
                epoch_secrets.secret_keys[i],
                &mut tree_cm,
            );

            coins.push(vec![coin]);
        }
        Ok(coins)
    }

    /// leadership reward, assuming constant reward
    /// TODO (res) implement reward mechanism with accord to DRK,DARK token-economics
    fn reward() -> u64 {
        constants::REWARD
    }

    /// Auxillary function to receive current slot offset.
    /// If offset is None, its setted up as last block slot offset.
    fn get_current_offset(&mut self) -> u64 {
        // This is the case were we restarted our node, didn't receive offset from other nodes,
        // so we need to find offset from last block
        if self.consensus.offset.is_none() {
            let last = self.blockchain.get_last_offset().unwrap();
            info!("overall_empty_slots(): Setting slot offset: {}", last);
            self.consensus.offset = Some(last);
        }

        self.consensus.offset.unwrap()
    }

    /// Auxillary function to calculate overall empty slots.
    /// We keep an offset from genesis indicating when the first slot actually started.
    /// This offset is shared between nodes.
    fn overall_empty_slots(&mut self) -> u64 {
        let slot = self.current_slot();
        // Retrieve existing blocks excluding genesis
        let blocks = (self.blockchain.len() as u64) - 1;
        // Setup offset if only have genesis and havent received offset from other nodes
        if blocks == 0 && self.consensus.offset.is_none() {
            info!(
                "overall_empty_slots(): Blockchain contains only genesis, setting slot offset: {}",
                slot
            );
            self.consensus.offset = Some(slot);
        }

        slot - blocks - self.get_current_offset()
    }

    /// total stake plus one.
    /// assuming constant Reward.
    fn total_stake_plus(&mut self, epoch: u64, slot: u64) -> i64 {
        ((epoch * constants::EPOCH_LENGTH as u64 + slot + 1 - self.overall_empty_slots()) *
            Self::reward()) as i64
    }

    /// Calculate how many leaders existed in previous slot and appends
    /// it to history, to report it if win. On finalization sync period,
    /// node replaces its leaders history with the sequence extracted by
    /// the longest fork.
    fn extend_leaders_history(&mut self) -> Float10 {
        let slot = self.current_slot();
        let previous_slot = slot - 1;
        let mut count = 0;
        for chain in &self.consensus.proposals {
            // Previous slot proposals exist at end of each fork
            if chain.proposals.last().unwrap().block.header.slot == previous_slot {
                count += 1;
            }
        }
        self.consensus.leaders_history.push(count);
        debug!(
            "extend_leaders_history(): Current leaders history: {:?}",
            self.consensus.leaders_history
        );
        Float10::try_from(count as i64).unwrap().with_precision(constants::RADIX_BITS).value()
    }

    fn f_dif(&mut self) -> Float10 {
        let one = constants::FLOAT10_ONE.clone();
        one - self.extend_leaders_history()
    }

    fn f_der(&self) -> Float10 {
        let len = self.consensus.leaders_history.len();
        let last = Float10::try_from(self.consensus.leaders_history[len - 1] as i64)
            .unwrap()
            .with_precision(constants::RADIX_BITS)
            .value();
        let second_to_last = Float10::try_from(self.consensus.leaders_history[len - 2] as i64)
            .unwrap()
            .with_precision(constants::RADIX_BITS)
            .value();
        (last - second_to_last) / constants::TD.clone()
    }

    fn f_int(&self) -> Float10 {
        let mut sum = constants::FLOAT10_ZERO.clone();
        for f in &self.consensus.leaders_history {
            sum += f.clone() * constants::TD.clone();
        }
        sum
    }

    /// the probability of winnig lottery having all the stake
    /// returns f
    fn win_prob_with_full_stake(&mut self) -> Float10 {
        let zero = constants::FLOAT10_ZERO.clone();
        let one = constants::FLOAT10_ONE.clone();
        let mut f = zero.clone();
        let step =
            Float10::from_str_native("0.1").unwrap().with_precision(constants::RADIX_BITS).value();
        let p = self.f_dif();
        let i = self.f_int();
        let d = self.f_der();
        info!("Consensus::win_prob_with_full_stake(): Kp: {}", self.consensus.kp.clone());
        while f <= zero || f >= one {
            f = self.consensus.kp.clone() *
                (p.clone() +
                    one.clone() / constants::TI.clone() * i.clone() +
                    constants::TD.clone() * d.clone());
            if f >= one {
                self.consensus.kp -= step.clone();
            } else if f <= zero {
                self.consensus.kp += step.clone();
            }
            info!("Consensus::win_prob_with_full_stake(): f: {}", f);
        }
        f
    }

    /// Check that the provided participant/stakeholder coins win the slot lottery.
    /// If the stakeholder has multiple competing winning coins, only the highest value
    /// coin is selected, since the stakeholder can't give more than one proof per block/slot.
    /// * `slot` - slot relative index
    /// * `epoch_coins` - stakeholder's epoch coins
    /// Returns: (check: bool, idx: usize) where idx is the winning coin's index
    pub fn is_slot_leader(&mut self) -> (bool, usize, pallas::Base, pallas::Base) {
        // Slot relative index
        let slot = self.relative_slot(self.current_slot());
        let (sigma1, sigma2) = self.sigmas(self.consensus.epoch, slot);
        // Stakeholder's epoch coins
        let coins = &self.consensus.coins;

        info!("Consensus::is_leader(): slot: {}, coins len: {}", slot, coins.len());
        assert!((slot as usize) < coins.len());

        let competing_coins = &coins[slot as usize];

        let mut won = false;
        let mut highest_stake = 0;
        let mut highest_stake_idx = 0;

        for (winning_idx, coin) in competing_coins.iter().enumerate() {
            let first_winning = coin.is_leader(sigma1, sigma2);
            if first_winning && !won {
                highest_stake_idx = winning_idx;
            }

            won |= first_winning;
            if won && coin.value > highest_stake {
                highest_stake = coin.value;
                highest_stake_idx = winning_idx;
            }
        }

        (won, highest_stake_idx, sigma1, sigma2)
    }

    /// Generate a block proposal for the current slot, containing all
    /// unconfirmed transactions. Proposal extends the longest fork
    /// chain the node is holding.
    pub fn propose(
        &mut self,
        idx: usize,
        sigma1: pallas::Base,
        sigma2: pallas::Base,
    ) -> Result<Option<BlockProposal>> {
        let slot = self.current_slot();
        let (prev_hash, index) = self.longest_chain_last_hash().unwrap();
        let unproposed_txs = self.unproposed_txs(index);

        // TODO: [PLACEHOLDER] Create and add rewards transaction

        let tree = BridgeTree::<MerkleNode, MERKLE_DEPTH>::new(100);
        /* TODO: FIXME: TESTNET:
        for tx in &unproposed_txs {
            for output in &tx.outputs {
                tree.append(&MerkleNode::from(output.revealed.coin.0));
                tree.witness();
            }
        }
        */
        let root = tree.root(0).unwrap();

        let eta = self.consensus.epoch_eta;
        // Generating leader proof
        let relative_slot = self.relative_slot(slot) as usize;
        let coin = self.consensus.coins[relative_slot][idx];
        let proof =
            coin.create_lead_proof(sigma1, sigma2, self.lead_proving_key.as_ref().unwrap())?;

        // Signing using coin
        let secret_key = coin.secret_key;
        let header =
            Header::new(prev_hash, self.slot_epoch(slot), slot, Timestamp::current_time(), root);
        let signed_proposal = secret_key.sign(&mut OsRng, &header.headerhash().as_bytes()[..]);
        let public_key = PublicKey::from_secret(secret_key);

        let lead_info = LeadInfo::new(
            signed_proposal,
            public_key,
            coin.public_inputs(),
            eta.to_repr(),
            LeadProof::from(proof),
            self.get_current_offset(),
            self.consensus.leaders_history.last().unwrap().clone(),
        );
        // Replacing old coin with the derived coin
        // TODO: do we need that? on next epoch we replace everything
        // how is this going to get reused?
        self.consensus.coins[relative_slot][idx] = coin.derive_coin();

        Ok(Some(BlockProposal::new(header, unproposed_txs, lead_info)))
    }

    /// Retrieve all unconfirmed transactions not proposed in previous blocks
    /// of provided index chain.
    pub fn unproposed_txs(&self, index: i64) -> Vec<Transaction> {
        let mut unproposed_txs = self.unconfirmed_txs.clone();

        // If index is -1 (canonical blockchain) a new fork will be generated,
        // therefore all unproposed transactions can be included in the proposal.
        if index == -1 {
            return unproposed_txs
        }

        // We iterate over the fork chain proposals to find already proposed
        // transactions and remove them from the local unproposed_txs vector.
        let chain = &self.consensus.proposals[index as usize];
        for proposal in &chain.proposals {
            for tx in &proposal.block.txs {
                if let Some(pos) = unproposed_txs.iter().position(|txs| *txs == *tx) {
                    unproposed_txs.remove(pos);
                }
            }
        }

        unproposed_txs
    }

    /// Finds the longest blockchain the node holds and
    /// returns the last block hash and the chain index.
    pub fn longest_chain_last_hash(&self) -> Result<(blake3::Hash, i64)> {
        let mut longest: Option<ProposalChain> = None;
        let mut length = 0;
        let mut index = -1;

        if !self.consensus.proposals.is_empty() {
            for (i, chain) in self.consensus.proposals.iter().enumerate() {
                if chain.proposals.len() > length {
                    longest = Some(chain.clone());
                    length = chain.proposals.len();
                    index = i as i64;
                }
            }
        }

        let hash = match longest {
            Some(chain) => chain.proposals.last().unwrap().hash,
            None => self.blockchain.last()?.1,
        };

        Ok((hash, index))
    }

    /// Given a proposal, the node verify its sender (slot leader) and finds which blockchain
    /// it extends. If the proposal extends the canonical blockchain, a new fork chain is created.
    pub async fn receive_proposal(&mut self, proposal: &BlockProposal) -> Result<()> {
        let current = self.current_slot();
        let coin_slot = &proposal.block.header.slot;
        let eta = self.consensus.epoch_eta;
        info!("Consensus::receive_proposal(): current slot: {}", current);
        info!("Consensus::receive_proposal(): proposed slot: {}", coin_slot);
        let (mu_y, mu_rho) = LeadCoin::election_seeds_u64(eta, *coin_slot);
        // Node hasn't started participating
        match self.consensus.participating {
            Some(start) => {
                if current < start {
                    return Ok(())
                }
            }
            None => return Ok(()),
        }

        // Node have already checked for finalization in this slot
        if current <= self.consensus.checked_finalization {
            warn!("receive_proposal(): Proposal received after finalization sync period.");
            return Err(Error::ProposalAfterFinalizationError)
        }

        let lf = &proposal.block.lead_info;
        let hdr = &proposal.block.header;

        // Verify proposal signature is valid based on producer public key
        // TODO: derive public key from proof
        if !lf.public_key.verify(proposal.header.as_bytes(), &lf.signature) {
            warn!("receive_proposal(): Proposer {} signature could not be verified", lf.public_key);
            return Err(Error::InvalidSignature)
        }

        // Check if proposal hash matches actual one
        let proposal_hash = proposal.block.blockhash();
        if proposal.hash != proposal_hash {
            warn!(
                "receive_proposal(): Received proposal contains mismatched hashes: {} - {}",
                proposal.hash, proposal_hash
            );
            return Err(Error::ProposalHashesMissmatchError)
        }

        // Check if proposal header matches actual one
        let proposal_header = hdr.headerhash();
        if proposal.header != proposal_header {
            warn!(
                "receive_proposal(): Received proposal contains mismatched headers: {} - {}",
                proposal.header, proposal_header
            );
            return Err(Error::ProposalHeadersMissmatchError)
        }

        // Verify proposal offset
        let offset = self.get_current_offset();
        if offset != lf.offset {
            warn!(
                "receive_proposal(): Received proposal contains different offset: {} - {}",
                offset, lf.offset
            );
            return Err(Error::ProposalDifferentOffsetError)
        }

        // Verify proposal leader proof
        if let Err(e) = lf.proof.verify(&self.lead_verifying_key, &lf.public_inputs) {
            error!("receive_proposal(): Error during leader proof verification: {}", e);
            return Err(Error::LeaderProofVerification)
        };
        info!("receive_proposal(): Leader proof verified successfully!");

        // verify proposal public values
        // mu values
        // y
        let prop_mu_y = lf.public_inputs[constants::PI_MU_Y_INDEX];
        if mu_y != prop_mu_y {
            error!("failed to verify mu_y: {:?}, proposed: {:?}", mu_y, prop_mu_y);
            return Err(Error::ProposalPublicValuesMismatched)
        }
        // rho
        let prop_mu_rho = lf.public_inputs[constants::PI_MU_RHO_INDEX];
        if mu_rho != prop_mu_rho {
            error!("failed to verify mu_rho: {:?}, proposed: {:?}", mu_rho, prop_mu_rho);
            return Err(Error::ProposalPublicValuesMismatched)
        }

        // Verify proposal public inputs
        let prop_sn = lf.public_inputs[constants::PI_NULLIFIER_INDEX];
        for sn in &self.consensus.leaders_nullifiers {
            if *sn == prop_sn {
                error!("receive_proposal(): Proposal nullifiers exist.");
                return Err(Error::ProposalIsSpent)
            }
        }
        let prop_cm_x: pallas::Base = lf.public_inputs[constants::PI_COMMITMENT_X_INDEX];
        let prop_cm_y: pallas::Base = lf.public_inputs[constants::PI_COMMITMENT_Y_INDEX];

        for cm in &self.consensus.leaders_spent_coins {
            if *cm == (prop_cm_x, prop_cm_y) {
                error!("receive_proposal(): Proposal coin already spent.");
                return Err(Error::ProposalIsSpent)
            }
        }

        // Check if proposal extends any existing fork chains
        let index = self.find_extended_chain_index(proposal)?;
        if index == -2 {
            return Err(Error::ExtendedChainIndexNotFound)
        }

        // Validate state transition against canonical state
        // TODO: This should be validated against fork state
        debug!("receive_proposal(): Starting state transition validation");
        if let Err(e) = self.verify_transactions(&proposal.block.txs, false).await {
            error!("receive_proposal(): Transaction verifications failed: {}", e);
            return Err(e.into())
        };

        // TODO: [PLACEHOLDER] Add rewards validation

        // Extend corresponding chain
        match index {
            -1 => {
                let pc = ProposalChain::new(self.consensus.genesis_block, proposal.clone());
                self.consensus.proposals.push(pc);
            }
            _ => {
                self.consensus.proposals[index as usize].add(proposal);
            }
        };

        // Store proposal coin info
        self.consensus.leaders_nullifiers.push(prop_sn);
        self.consensus.leaders_spent_coins.push((prop_cm_x, prop_cm_y));

        Ok(())
    }

    /// Given a proposal, find the index of the fork chain it extends.
    pub fn find_extended_chain_index(&mut self, proposal: &BlockProposal) -> Result<i64> {
        // We iterate through all forks to find which fork to extend
        let mut chain_index = -1;
        let mut prop_index = 0;
        for (c_index, chain) in self.consensus.proposals.iter().enumerate() {
            // Traverse proposals in reverse
            for (p_index, prop) in chain.proposals.iter().enumerate().rev() {
                if proposal.block.header.previous == prop.hash {
                    chain_index = c_index as i64;
                    prop_index = p_index;
                    break
                }
            }
            if chain_index != -1 {
                break
            }
        }

        // If no fork was found, we check with canonical
        if chain_index == -1 {
            let (last_slot, last_block) = self.blockchain.last()?;
            if proposal.block.header.previous != last_block ||
                proposal.block.header.slot <= last_slot
            {
                debug!("find_extended_chain_index(): Proposal doesn't extend any known chain");
                return Ok(-2)
            }

            // Proposal extends canonical chain
            return Ok(-1)
        }

        // Found fork chain
        let chain = &self.consensus.proposals[chain_index as usize];
        // Proposal extends fork at last proposal
        if prop_index == (chain.proposals.len() - 1) {
            return Ok(chain_index)
        }

        debug!("find_extended_chain_index(): Proposal to fork a forkchain was received.");
        let mut chain = self.consensus.proposals[chain_index as usize].clone();
        // We keep all proposals until the one it extends
        chain.proposals.drain((prop_index + 1)..);
        self.consensus.proposals.push(chain);
        Ok(self.consensus.proposals.len() as i64 - 1)
    }

    /// Search the chains we're holding for the given proposal.
    pub fn proposal_exists(&self, input_proposal: &blake3::Hash) -> bool {
        for chain in self.consensus.proposals.iter() {
            for proposal in chain.proposals.iter() {
                if input_proposal == &proposal.hash {
                    return true
                }
            }
        }

        false
    }

    /// Remove provided transactions vector from unconfirmed_txs if they exist.
    pub fn remove_txs(&mut self, transactions: Vec<Transaction>) -> Result<()> {
        for tx in transactions {
            if let Some(pos) = self.unconfirmed_txs.iter().position(|txs| *txs == tx) {
                self.unconfirmed_txs.remove(pos);
            }
        }

        Ok(())
    }

    /// Auxillary function to set nodes leaders count history to the largest fork sequence
    /// of leaders, by using provided index.
    fn set_leader_history(&mut self, index: i64) {
        // Check if we found longest fork to extract sequence from
        match index {
            -1 => {
                debug!("set_leader_history(): No fork exists.");
            }
            _ => {
                debug!("set_leader_history(): Checking last proposal of fork: {}", index);
                let last_proposal =
                    self.consensus.proposals[index as usize].proposals.last().unwrap();
                if last_proposal.block.header.slot == self.current_slot() {
                    // Replacing our last history element with the leaders one
                    self.consensus.leaders_history.pop();
                    self.consensus.leaders_history.push(last_proposal.block.lead_info.leaders);
                    debug!(
                        "set_leader_history(): New leaders history: {:?}",
                        self.consensus.leaders_history
                    );
                    return
                }
            }
        }
        self.consensus.leaders_history.push(0);
    }

    /// Node checks if any of the fork chains can be finalized.
    /// Consensus finalization logic:
    /// - If the node has observed the creation of 3 proposals in a fork chain and no other
    ///   forks exists at same or greater height, it finalizes (appends to canonical blockchain)
    ///   all proposals up to the last one.
    /// When fork chain proposals are finalized, the rest of fork chains are removed.
    pub async fn chain_finalization(&mut self) -> Result<Vec<BlockInfo>> {
        let slot = self.current_slot();
        debug!("chain_finalization(): Started finalization check for slot: {}", slot);
        // Set last slot finalization check occured to current slot
        self.consensus.checked_finalization = slot;

        // First we find longest chain without any other forks at same height
        let mut chain_index = -1;
        // Use this index to extract leaders count sequence from longest fork
        let mut index_for_history = -1;
        let mut max_length = 0;
        for (index, chain) in self.consensus.proposals.iter().enumerate() {
            let length = chain.proposals.len();
            // Check if greater than max to retain index for history
            if length > max_length {
                index_for_history = index as i64;
            }
            // Ignore forks with less that 3 blocks
            if length < 3 {
                continue
            }
            // Check if less than max
            if length < max_length {
                continue
            }
            // Check if same length as max
            if length == max_length {
                // Setting chain_index so we know we have multiple
                // forks at same length.
                chain_index = -2;
                continue
            }
            // Set chain as max
            chain_index = index as i64;
            max_length = length;
        }

        // Check if we found any fork to finalize
        match chain_index {
            -2 => {
                debug!("chain_finalization(): Eligible forks with same height exist, nothing to finalize.");
                self.set_leader_history(index_for_history);
                return Ok(vec![])
            }
            -1 => {
                debug!("chain_finalization(): All chains have less than 3 proposals, nothing to finalize.");
                self.set_leader_history(index_for_history);
                return Ok(vec![])
            }
            _ => debug!("chain_finalization(): Chain {} can be finalized!", chain_index),
        }

        // Starting finalization
        let mut chain = self.consensus.proposals[chain_index as usize].clone();

        // Retrieving proposals to finalize
        let bound = max_length - 1;
        let mut finalized: Vec<BlockInfo> = vec![];
        for proposal in &chain.proposals[..bound] {
            finalized.push(proposal.clone().into());
        }

        // Removing finalized proposals from chain
        chain.proposals.drain(..bound);

        // Adding finalized proposals to canonical
        info!("consensus: Adding {} finalized block to canonical chain.", finalized.len());
        match self.blockchain.add(&finalized) {
            Ok(v) => v,
            Err(e) => {
                error!("consensus: Failed appending finalized blocks to canonical chain: {}", e);
                return Err(e)
            }
        };

        let blocks_subscriber = self.subscribers.get("blocks").unwrap();

        // Validating state transitions
        for proposal in &finalized {
            // TODO: Is this the right place? We're already doing this in protocol_sync.
            // TODO: These state transitions have already been checked. (I wrote this, but where?)
            // TODO: FIXME: The state transitions have already been written, they have to be in memory
            //              until this point.
            debug!(target: "consensus", "Applying state transition for finalized block");
            if let Err(e) = self.verify_transactions(&proposal.txs, true).await {
                error!(target: "consensus", "Finalized block transaction verifications failed: {}", e);
                return Err(e)
            }

            // TODO: Don't hardcode this:
            let params = json!([bs58::encode(&serialize(proposal)).into_string()]);
            let notif = JsonNotification::new("blockchain.subscribe_blocks", params);
            info!("consensus: Sending notification about finalized block");
            blocks_subscriber.notify(notif).await;
        }

        // Setting leaders history to last proposal leaders count
        self.consensus.leaders_history =
            vec![chain.proposals.last().unwrap().block.lead_info.leaders];

        // Removing rest forks
        self.consensus.proposals = vec![];
        self.consensus.proposals.push(chain);

        Ok(finalized)
    }

    /// Utility function to extract leader selection lottery randomness(eta),
    /// defined as the hash of the previous lead proof converted to pallas base.
    fn get_eta(&self) -> pallas::Base {
        let proof_tx_hash = self.blockchain.get_last_proof_hash().unwrap();
        let mut bytes: [u8; 32] = *proof_tx_hash.as_bytes();
        // read first 254 bits
        bytes[30] = 0;
        bytes[31] = 0;
        pallas::Base::from_repr(bytes).unwrap()
    }

    // ==========================
    // State transition functions
    // ==========================
    // TODO TESTNET: Write down all cases below
    // State transition checks should be happening in the following cases for a sync node:
    // 1) When a finalized block is received
    // 2) When a transaction is being broadcasted to us
    // State transition checks should be happening in the following cases for a consensus participating node:
    // 1) When a finalized block is received
    // 2) When a transaction is being broadcasted to us
    // ==========================

    /// Validate and append to canonical state received blocks.
    pub async fn receive_blocks(&mut self, blocks: &[BlockInfo]) -> Result<()> {
        // Verify state transitions for all blocks and their respective transactions.
        debug!("receive_blocks(): Starting state transition validations");
        for block in blocks {
            if let Err(e) = self.verify_transactions(&block.txs, false).await {
                error!("receive_blocks(): Transaction verifications failed: {}", e);
                return Err(e)
            }
        }

        debug!("receive_blocks(): All state transitions passed");
        debug!("receive_blocks(): Appending blocks to ledger");
        self.blockchain.add(blocks)?;

        Ok(())
    }

    /// Validate and append to canonical state received finalized block.
    /// Returns boolean flag indicating already existing block.
    pub async fn receive_finalized_block(&mut self, block: BlockInfo) -> Result<bool> {
        match self.blockchain.has_block(&block) {
            Ok(v) => {
                if v {
                    debug!("receive_finalized_block(): Existing block received");
                    return Ok(false)
                }
            }
            Err(e) => {
                error!("receive_finalized_block(): failed checking for has_block(): {}", e);
                return Ok(false)
            }
        };

        debug!("receive_finalized_block(): Executing state transitions");
        self.receive_blocks(&[block.clone()]).await?;

        // TODO: Don't hardcode this:
        let blocks_subscriber = self.subscribers.get("blocks").unwrap();
        let params = json!([bs58::encode(&serialize(&block)).into_string()]);
        let notif = JsonNotification::new("blockchain.subscribe_blocks", params);
        info!("consensus: Sending notification about finalized block");
        blocks_subscriber.notify(notif).await;

        debug!("receive_finalized_block(): Removing block transactions from unconfirmed_txs");
        self.remove_txs(block.txs.clone())?;

        Ok(true)
    }

    /// Validate and append to canonical state received finalized blocks from block sync task.
    /// Already existing blocks are ignored.
    pub async fn receive_sync_blocks(&mut self, blocks: &[BlockInfo]) -> Result<()> {
        let mut new_blocks = vec![];
        for block in blocks {
            match self.blockchain.has_block(block) {
                Ok(v) => {
                    if v {
                        debug!("receive_sync_blocks(): Existing block received");
                        continue
                    }
                    new_blocks.push(block.clone());
                }
                Err(e) => {
                    error!("receive_sync_blocks(): failed checking for has_block(): {}", e);
                    continue
                }
            };
        }

        if new_blocks.is_empty() {
            debug!("receive_sync_blocks(): no new blocks to append");
            return Ok(())
        }

        debug!("receive_sync_blocks(): Executing state transitions");
        self.receive_blocks(&new_blocks[..]).await?;

        // TODO: Don't hardcode this:
        let blocks_subscriber = self.subscribers.get("blocks").unwrap();
        for block in new_blocks {
            let params = json!([bs58::encode(&serialize(&block)).into_string()]);
            let notif = JsonNotification::new("blockchain.subscribe_blocks", params);
            info!("consensus: Sending notification about finalized block");
            blocks_subscriber.notify(notif).await;
        }

        Ok(())
    }

    /// Validate signatures, wasm execution, and zk proofs for given transactions.
    /// If all of those succeed, try to execute a state update for the contract calls.
    /// Currently the verifications are sequential, and the function will fail if any
    /// of the verifications fail.
    /// The function takes a boolean called `write` which tells it to actually write
    /// the state transitions to the database.
    // TODO: This should be paralellized as if even one tx in the batch fails to verify,
    //       we can drop everything.
    pub async fn verify_transactions(&self, txs: &[Transaction], write: bool) -> Result<()> {
        debug!("Verifying {} transaction(s)", txs.len());
        for tx in txs {
            let tx_hash = blake3::hash(&serialize(tx));
            debug!("Verifying transaction {}", tx_hash);

            // Table of public inputs used for ZK proof verification
            let mut zkp_table = vec![];
            // Table of public keys used for signature verification
            let mut sig_table = vec![];
            // State updates produced by contract execcution
            let mut updates = vec![];

            // Iterate over all calls to get the metadata
            for (idx, call) in tx.calls.iter().enumerate() {
                debug!("Executing contract call {}", idx);
                let wasm = match self.blockchain.wasm_bincode.get(call.contract_id) {
                    Ok(v) => {
                        debug!("Found wasm bincode for {}", call.contract_id);
                        v
                    }
                    Err(e) => {
                        error!(
                            "Could not find wasm bincode for contract {}: {}",
                            call.contract_id, e
                        );
                        return Err(Error::ContractNotFound(call.contract_id.to_string()))
                    }
                };

                // Write the actual payload data
                let mut payload = vec![];
                payload.write_u32(idx as u32)?; // Call index
                tx.calls.encode(&mut payload)?; // Actual call data

                // Instantiate the wasm runtime
                let mut runtime =
                    match Runtime::new(&wasm, self.blockchain.clone(), call.contract_id) {
                        Ok(v) => v,
                        Err(e) => {
                            error!(
                                "Failed to instantiate WASM runtime for contract {}",
                                call.contract_id
                            );
                            return Err(e.into())
                        }
                    };

                debug!("Executing \"metadata\" call");
                let metadata = match runtime.metadata(&payload) {
                    Ok(v) => v,
                    Err(e) => {
                        error!("Failed to execute \"metadata\" call: {}", e);
                        return Err(e.into())
                    }
                };

                // Decode the metadata retrieved from the execution
                let mut decoder = Cursor::new(&metadata);
                let zkp_pub: Vec<(String, Vec<pallas::Base>)> =
                    match Decodable::decode(&mut decoder) {
                        Ok(v) => v,
                        Err(e) => {
                            error!("Failed to decode ZK public inputs from metadata: {}", e);
                            return Err(e.into())
                        }
                    };

                let sig_pub: Vec<PublicKey> = match Decodable::decode(&mut decoder) {
                    Ok(v) => v,
                    Err(e) => {
                        error!("Failed to decode signature pubkeys from metadata: {}", e);
                        return Err(e.into())
                    }
                };

                // TODO: Make sure we've read all the bytes above.
                debug!("Successfully executed \"metadata\" call");
                zkp_table.push(zkp_pub);
                sig_table.push(sig_pub);

                // After getting the metadata, we run the "exec" function with the same
                // runtime and the same payload.
                debug!("Executing \"exec\" call");
                match runtime.exec(&payload) {
                    Ok(v) => {
                        debug!("Successfully executed \"exec\" call");
                        updates.push(v);
                    }
                    Err(e) => {
                        error!(
                            "Failed to execute \"exec\" call for contract id {}: {}",
                            call.contract_id, e
                        );
                        return Err(e.into())
                    }
                };
                // At this point we're done with the call and move on to the next one.
            }

            // When we're done looping and executing over the tx's contract calls, we
            // move on with verification. First we verify the signatures as that's
            // cheaper, and then finally we verify the ZK proofs.
            debug!("Verifying signatures for transaction {}", tx_hash);
            match tx.verify_sigs(sig_table) {
                Ok(()) => debug!("Signatures verification for tx {} successful", tx_hash),
                Err(e) => {
                    error!("Signature verification for tx {} failed: {}", tx_hash, e);
                    return Err(e.into())
                }
            };

            // NOTE: When it comes to the ZK proofs, we first do a lookup of the
            // verifying keys, but if we do not find them, we'll generate them
            // inside of this function. This can be kinda expensive, so open to
            // alternatives.
            debug!("Verifying ZK proofs for transaction {}", tx_hash);
            match tx.verify_zkps(self.verifying_keys.clone(), zkp_table).await {
                Ok(()) => debug!("ZK proof verification for tx {} successful", tx_hash),
                Err(e) => {
                    error!("ZK proof verrification for tx {} failed: {}", tx_hash, e);
                    return Err(e.into())
                }
            };

            // After the verifications stage passes, if we're told to write, we
            // apply the state updates.
            assert!(tx.calls.len() == updates.len());
            if write {
                debug!("Performing state updates");
                for (call, update) in tx.calls.iter().zip(updates.iter()) {
                    // For this we instantiate the runtimes again.
                    // TODO: Optimize this
                    // TODO: Sum up the gas costs of previous calls during execution
                    //       and verification and these.
                    let wasm = match self.blockchain.wasm_bincode.get(call.contract_id) {
                        Ok(v) => {
                            debug!("Found wasm bincode for {}", call.contract_id);
                            v
                        }
                        Err(e) => {
                            error!(
                                "Could not find wasm bincode for contract {}: {}",
                                call.contract_id, e
                            );
                            return Err(Error::ContractNotFound(call.contract_id.to_string()))
                        }
                    };

                    let mut runtime =
                        match Runtime::new(&wasm, self.blockchain.clone(), call.contract_id) {
                            Ok(v) => v,
                            Err(e) => {
                                error!(
                                    "Failed to instantiate WASM runtime for contract {}",
                                    call.contract_id
                                );
                                return Err(e.into())
                            }
                        };

                    debug!("Executing \"apply\" call");
                    match runtime.apply(&update) {
                        // TODO: FIXME: This should be done in an atomic tx/batch
                        Ok(()) => debug!("State update applied successfully"),
                        Err(e) => {
                            error!("Failed to apply state update: {}", e);
                            return Err(e.into())
                        }
                    };
                }
            } else {
                debug!("Skipping apply of state updates because write=false");
            }

            debug!("Transaction {} verified successfully", tx_hash);
        }

        Ok(())
    }
}
