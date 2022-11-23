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

use std::{io::Cursor, time::Duration};

use async_std::sync::{Arc, RwLock};
use chrono::{NaiveDateTime, Utc};
use darkfi_sdk::crypto::{
    constants::MERKLE_DEPTH,
    pedersen::pedersen_commitment_base,
    poseidon_hash,
    schnorr::{SchnorrPublic, SchnorrSecret},
    util::mod_r_p,
    ContractId, MerkleNode, PublicKey,
};
use darkfi_serial::{serialize, Decodable, Encodable, SerialDecodable, SerialEncodable, WriteExt};
use incrementalmerkletree::{bridgetree::BridgeTree, Tree};
use log::{debug, error, info, warn};
use pasta_curves::{
    arithmetic::CurveAffine,
    group::{ff::PrimeField, Curve},
    pallas,
};
use rand::{rngs::OsRng, thread_rng, Rng};

use super::{
    constants::{
        EPOCH_LENGTH, LEADER_PROOF_K, LOTTERY_HEAD_START, P, RADIX_BITS, REWARD, SLOT_TIME,
    },
    leadcoin::{LeadCoin, LeadCoinSecrets},
    utils::fbig2base,
    Block, BlockInfo, BlockProposal, Float10, Header, LeadProof, Metadata, ProposalChain,
};

use crate::{
    blockchain::Blockchain,
    crypto::proof::{ProvingKey, VerifyingKey},
    net,
    runtime::vm_runtime::Runtime,
    tx::Transaction,
    util::time::Timestamp,
    wallet::WalletPtr,
    zk::{vm::ZkCircuit, vm_stack::empty_witnesses},
    zkas::ZkBinary,
    Error, Result,
};

const PI_NULLIFIER_INDEX: usize  = 7;
const PI_COMMITMENT_X_INDEX: usize = 1;
const PI_COMMITMENT_Y_INDEX: usize = 2;
/// This struct represents the information required by the consensus algorithm
#[derive(Debug)]
pub struct ConsensusState {
    /// Genesis block creation timestamp
    pub genesis_ts: Timestamp,
    /// Genesis block hash
    pub genesis_block: blake3::Hash,
    /// Fork chains containing block proposals
    pub proposals: Vec<ProposalChain>,
    /// Current epoch
    pub epoch: u64,
    /// Current epoch eta
    pub epoch_eta: pallas::Base,
    /// Current epoch competing coins
    pub coins: Vec<Vec<LeadCoin>>,
}

impl ConsensusState {
    pub fn new(genesis_ts: Timestamp, genesis_data: blake3::Hash) -> Result<Self> {
        let genesis_block = Block::genesis_block(genesis_ts, genesis_data).blockhash();

        Ok(Self {
            genesis_ts,
            genesis_block,
            proposals: vec![],
            epoch: 0,
            epoch_eta: pallas::Base::one(),
            coins: vec![],
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
    /// Hot/live data used by the consensus algorithm
    pub proposals: Vec<ProposalChain>,
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
    pub lead_proving_key: ProvingKey,
    /// Leader proof verifying key
    pub lead_verifying_key: VerifyingKey,
    /// Hot/Live data used by the consensus algorithm
    pub consensus: ConsensusState,
    /// Canonical (finalized) blockchain
    pub blockchain: Blockchain,
    /// Pending transactions
    pub unconfirmed_txs: Vec<Transaction>,
    /// Participating start slot
    pub participating: Option<u64>,
    /// Wallet interface
    pub wallet: WalletPtr,
    /// nullifiers
    pub nullifiers: Vec<pallas::Base>,
    /// spent coins
    pub spent: Vec<(pallas::Base, pallas::Base)>,
    /// lead coins
    pub lead: Vec<(pallas::Base, pallas::Base)>,
    /// f history
    pub f_history : Vec<Float10>,
    /// Kp
    pub Kp: Float10,
    /// Ti
    pub Ti: Float10,
    /// Td
    pub Td: Float10,
}

impl ValidatorState {
    pub async fn new(
        db: &sled::Db, // <-- TODO: Avoid this with some wrapping, sled should only be in blockchain
        genesis_ts: Timestamp,
        genesis_data: blake3::Hash,
        wallet: WalletPtr,
        faucet_pubkeys: Vec<PublicKey>,
    ) -> Result<ValidatorStatePtr> {
        info!("Initializing ValidatorState");

        info!("Initializing wallet tables for consensus");
        // TODO: TESTNET: The stuff is kept entirely in memory for now, what should we write
        //                to disk/wallet?
        //let consensus_tree_init_query = include_str!("../../script/sql/consensus_tree.sql");
        //let consensus_keys_init_query = include_str!("../../script/sql/consensus_keys.sql");
        //wallet.exec_sql(consensus_tree_init_query).await?;
        //wallet.exec_sql(consensus_keys_init_query).await?;

        info!("Generating leader proof keys with k: {}", LEADER_PROOF_K);
        let bincode = include_bytes!("../../proof/lead.zk.bin");
        let zkbin = ZkBinary::decode(bincode)?;
        let void_witnesses = empty_witnesses(&zkbin);
        let circuit = ZkCircuit::new(void_witnesses, zkbin);
        let lead_proving_key = ProvingKey::build(LEADER_PROOF_K, &circuit);
        let lead_verifying_key = VerifyingKey::build(LEADER_PROOF_K, &circuit);

        let consensus = ConsensusState::new(genesis_ts, genesis_data)?;
        let blockchain = Blockchain::new(db, genesis_ts, genesis_data)?;
        let unconfirmed_txs = vec![];
        let participating = None;

        // -----BEGIN ARTIFACT: WASM INTEGRATION-----
        // This is the current place where this stuff is being done, and very loosely.
        // We initialize and "deploy" _native_ contracts here - currently the money contract.
        // Eventually, the crypsinous consensus should be a native contract like payments are.
        // This means the previously existing Blockchain state will be a bit different and is
        // going to have to be changed.
        // When the `Blockchain` object is created, it doesn't care whether it already has
        // data or not. If there's existing data it will just open the necessary db and trees,
        // and give back what it has. This means, on subsequent runs our native contracts will
        // already be in a deployed state. So what we do here is a "re-deployment". This kind
        // of operation should only modify the contract's state in case it wasn't deployed
        // before (meaning the initial run). Otherwise, it shouldn't touch anything, or just
        // potentially update the database schemas or whatever is necessary. Here it's
        // transparent and generic, and the entire logic for this db protection is supposed to
        // be in the `init` function of the contract, so look there for a reference of the
        // databases and the state.
        info!("ValidatorState::new(): Deploying \"money_contract.wasm\"");
        let money_contract_wasm_bincode = include_bytes!("../contract/money/money_contract.wasm");
        // XXX: FIXME: This ID should be something that does not solve the pallas curve equation,
        //             and/or just hardcoded and forbidden in non-native contract deployment.
        let cid = ContractId::from(pallas::Base::from(u64::MAX - 420));
        let mut runtime = Runtime::new(&money_contract_wasm_bincode[..], blockchain.clone(), cid)?;
        // The faucet pubkeys are pubkeys which are allowed to create clear inputs in the
        // money contract.
        let payload = serialize(&faucet_pubkeys);
        runtime.deploy(&payload)?;
        info!("Deployed Money Contract with ID: {}", cid);
        // -----END ARTIFACT-----
        let zero = Float10::from_str_native("0").unwrap().with_precision(RADIX_BITS).value();
        let one = Float10::from_str_native("1").unwrap().with_precision(RADIX_BITS).value();
        let ten = Float10::from_str_native("10").unwrap().with_precision(RADIX_BITS).value();
        let three = Float10::from_str_native("3").unwrap().with_precision(RADIX_BITS).value();
        let nine  = Float10::from_str_native("9").unwrap().with_precision(RADIX_BITS).value();
        let state = Arc::new(RwLock::new(ValidatorState {
            lead_proving_key,
            lead_verifying_key,
            consensus,
            blockchain,
            unconfirmed_txs,
            participating,
            wallet,
            nullifiers: vec![],
            spent: vec![],
            lead: vec![],
            f_history: vec![zero],
            Kp: three/nine,
            Ti: one.clone()/ten.clone(),
            Td: one/ten,
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
        if let Err(e) = self.verify_transactions(&[tx.clone()]) {
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
        slot / EPOCH_LENGTH as u64
    }

    /// Calculates current slot, based on elapsed time from the genesis block.
    /// Slot duration is configured using the `SLOT_TIME` constant.
    pub fn current_slot(&self) -> u64 {
        self.consensus.genesis_ts.elapsed() / SLOT_TIME
    }

    /// Calculates the relative number of the provided slot.
    pub fn relative_slot(&self, slot: u64) -> u64 {
        slot % EPOCH_LENGTH as u64
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
        let next_slot_start = (current_slot * SLOT_TIME) + (start_time.timestamp() as u64);
        let next_slot_start = NaiveDateTime::from_timestamp(next_slot_start as i64, 0);
        let current_time = NaiveDateTime::from_timestamp(Utc::now().timestamp(), 0);
        let diff = next_slot_start - current_time;

        Duration::new(diff.num_seconds().try_into().unwrap(), 0)
    }

    /// Calculate slots until next Nth epoch.
    /// Epoch duration is configured using the EPOCH_LENGTH value.
    pub fn slots_to_next_n_epoch(&self, n: u64) -> u64 {
        assert!(n > 0);
        let slots_till_next_epoch = EPOCH_LENGTH as u64 - self.relative_slot(self.current_slot());
        ((n - 1) * EPOCH_LENGTH as u64) + slots_till_next_epoch
    }

    /// Calculates seconds until next Nth epoch starting time.
    pub fn next_n_epoch_start(&self, n: u64) -> Duration {
        self.next_n_slot_start(self.slots_to_next_n_epoch(n))
    }

    /// Set participating slot to next.
    pub fn set_participating(&mut self) -> Result<()> {
        self.participating = Some(self.current_slot() + 1);
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
        self.consensus.coins = self.create_epoch_coins(eta, epoch, 0).await?;
        self.consensus.epoch = epoch;
        self.consensus.epoch_eta = eta;
        Ok(true)
    }

    /// return 2-term target approximation sigma coefficients.
    /// `epoch: absolute epoch index
    /// `slot: relative slot index
    fn sigmas(&mut self, epoch: u64, slot: u64) -> (pallas::Base, pallas::Base) {
        //let f = Self::leadership_probability_with_all_stake().with_precision(RADIX_BITS).value();
        let f = self.win_prob_with_full_stake();
        info!("Consensus: f: {}", f);

        // Generate sigmas
        let total_stake = Self::total_stake_plus(epoch, slot); // Only used for fine-tuning

        let one = Float10::from_str_native("1").unwrap().with_precision(RADIX_BITS).value();
        let two = Float10::from_str_native("2").unwrap().with_precision(RADIX_BITS).value();
        let field_p = Float10::from_str_native(P).unwrap().with_precision(RADIX_BITS).value();
        let total_sigma =
            Float10::try_from(total_stake).unwrap().with_precision(RADIX_BITS).value();

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
        slot: u64,
    ) -> Result<Vec<Vec<LeadCoin>>> {
        info!("Consensus: Creating coins for epoch: {}", epoch);
        self.create_coins(eta).await
    }

    /// Generate coins for provided sigmas.
    /// NOTE: The strategy here is having a single competing coin per slot.
    async fn create_coins(
        &self,
        eta: pallas::Base,
    ) -> Result<Vec<Vec<LeadCoin>>> {
        let mut rng = thread_rng();

        let mut seeds: Vec<u64> = Vec::with_capacity(EPOCH_LENGTH);
        for _ in 0..EPOCH_LENGTH {
            seeds.push(rng.gen());
        }

        let epoch_secrets = LeadCoinSecrets::generate();

        let mut tree_cm = BridgeTree::<MerkleNode, MERKLE_DEPTH>::new(EPOCH_LENGTH);
        // LeadCoin matrix where each row represents a slot and contains its competing coins.
        let mut coins: Vec<Vec<LeadCoin>> = Vec::with_capacity(EPOCH_LENGTH);

        // TODO: TESTNET: Here we would look into the wallet to find coins we're able to use.
        //                The wallet has specific tables for consensus coins.
        // TODO: TESTNET: Token ID still has to be enforced properly in the consensus.

        // Temporarily, we compete with zero stake
        for i in 0..EPOCH_LENGTH {
            let coin = LeadCoin::new(
                eta,
                LOTTERY_HEAD_START, // TODO: TESTNET: Why is this constant being used?
                i as u64,
                epoch_secrets.secret_keys[i].inner(),
                epoch_secrets.merkle_roots[i],
                i, //TODO same as idx now for simplicity.
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
        REWARD
    }

    ///TODO: impl total empty slots count.
    fn empty_slots_count() -> u64 {
        0
    }

    /// total stake plus one.
    /// assuming constant Reward.
    fn total_stake_plus(epoch: u64, slot: u64) -> u64 {
        (epoch * EPOCH_LENGTH as u64 + slot + 1 - Self::empty_slots_count()) * Self::reward()
    }

    /// get number of leaders in last epoch.
    /// in ideal-world all nodes will end up with identical POV
    /// of blockchain, but in real-world:
    /// TODO: this parameter need to be published in the block header,
    /// and only read from last block header.
    fn leads_per_block(&mut self) -> Float10 {
        //TODO: complete this
        let fi64 : i64 = 1;
        self.f_history.push(Float10::try_from(fi64).unwrap().with_precision(RADIX_BITS).value());
        Float10::try_from(fi64).unwrap().with_precision(RADIX_BITS).value()
    }

    fn f_dif(&mut self) -> Float10 {
        let one = Float10::from_str_native("1").unwrap().with_precision(RADIX_BITS).value();
        one - self.leads_per_block()
    }

    fn f_der(&self) -> Float10 {
        let len = self.f_history.len();
        (self.f_history[len-1].clone() - self.f_history[len-2].clone())/self.Td.clone()
    }

    fn f_int(&self) -> Float10 {
        let mut sum  = Float10::from_str_native("0").unwrap().with_precision(RADIX_BITS).value();;
        for f in &self.f_history {
            sum += f.clone()*self.Td.clone();
        }
        sum
    }

    /// the probability of winnig lottery having all the stake
    /// returns f
    fn win_prob_with_full_stake(&mut self) -> Float10 {
        let one = Float10::from_str_native("1").unwrap().with_precision(RADIX_BITS).value();
        let zero = Float10::from_str_native("0").unwrap().with_precision(RADIX_BITS).value();
        let mut f = zero.clone();
        let step = Float10::from_str_native("0.1").unwrap().with_precision(RADIX_BITS).value();;
        let p = self.f_dif();
        let i = self.f_int();
        let d = self.f_der();
        while f<=Float10::from_str_native("0").unwrap().with_precision(RADIX_BITS).value() && f>=Float10::from_str_native("1").unwrap().with_precision(RADIX_BITS).value() {
            f = self.Kp.clone()*(p.clone() + one.clone()/self.Ti.clone() * i.clone() + self.Td.clone() * d.clone());
            if f>= one {
                self.Kp-=step.clone();
            } else if f<=zero {
                self.Kp+=step.clone();
            }
        }
        Float10::try_from(f).unwrap().with_precision(RADIX_BITS).value()
    }

    /// Check that the provided participant/stakeholder coins win the slot lottery.
    /// If the stakeholder has multiple competing winning coins, only the highest value
    /// coin is selected, since the stakeholder can't give more than one proof per block/slot.
    /// * `slot` - slot relative index
    /// * `epoch_coins` - stakeholder's epoch coins
    /// Returns: (check: bool, idx: usize) where idx is the winning coin's index
    pub fn is_slot_leader(&mut self) -> (bool, usize) {
        // Slot relative index
        let slot = self.relative_slot(self.current_slot());
        let epoch = self.current_epoch();
        let (sigma1, sigma2) = self.sigmas(slot, epoch);
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

        (won, highest_stake_idx)
    }

    /// Generate a block proposal for the current slot, containing all
    /// unconfirmed transactions. Proposal extends the longest fork
    /// chain the node is holding.
    pub fn propose(&mut self, idx: usize) -> Result<Option<BlockProposal>> {
        let slot = self.current_slot();
        let epoch = self.current_epoch();
        let (sigma1, sigma2) = self.sigmas(slot, epoch);
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
        let proof = coin.create_lead_proof(sigma1, sigma2, &self.lead_proving_key)?;

        // Signing using coin
        let secret_key = coin.secret_key;
        let header =
            Header::new(prev_hash, self.slot_epoch(slot), slot, Timestamp::current_time(), root);
        let signed_proposal = secret_key.sign(&mut OsRng, &header.headerhash().as_bytes()[..]);
        let public_key = PublicKey::from_secret(secret_key);

        let metadata = Metadata::new(
            signed_proposal,
            public_key,
            coin.public_inputs(),
            eta.to_repr(),
            LeadProof::from(proof),
        );
        // Replacing old coin with the derived coin
        // TODO: do we need that? on next epoch we replace everything
        // how is this going to get reused?
        self.consensus.coins[relative_slot][idx] = coin.derive_coin(eta, relative_slot as u64);

        /// lead,spend,nullifiers
        self.nullifiers.push(coin.sn());
        let cm = coin.coin1_commitment.to_affine().coordinates().unwrap();
        self.spent.push((*cm.x(), *cm.y()));
        self.lead.push((*cm.x(), *cm.y()));

        Ok(Some(BlockProposal::new(header, unproposed_txs, metadata)))
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
        //TODO: validate sn/cm
        let prop_sn = proposal.block.metadata.public_inputs[PI_NULLIFIER_INDEX];
        for sn in &self.nullifiers {
            if *sn==prop_sn {
                return Err(Error::ProposalIsSpent);
            }
        }
        let prop_cm_x : pallas::Base = proposal.block.metadata.public_inputs[PI_COMMITMENT_X_INDEX];
        let prop_cm_y : pallas::Base = proposal.block.metadata.public_inputs[PI_COMMITMENT_Y_INDEX];

        for cm in &self.lead {
            if *cm==(prop_cm_x, prop_cm_y) {
                return Err(Error::ProposalIsSpent);
            }
        }
        let current = self.current_slot();
        // Node hasn't started participating
        match self.participating {
            Some(start) => {
                if current < start {
                    return Ok(())
                }
            }
            None => return Ok(()),
        }

        let md = &proposal.block.metadata;
        let hdr = &proposal.block.header;

        // Verify proposal signature is valid based on producer public key
        // TODO: derive public key from proof
        if !md.public_key.verify(proposal.header.as_bytes(), &md.signature) {
            warn!("receive_proposal(): Proposer {} signature could not be verified", md.public_key);
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

        // Verify proposal leader proof
        if let Err(e) = md.proof.verify(&self.lead_verifying_key, &md.public_inputs) {
            error!("receive_proposal(): Error during leader proof verification: {}", e);
            return Err(Error::LeaderProofVerification)
        };
        info!("receive_proposal(): Leader proof verified successfully!");

        // TODO: Verify proposal public inputs

        // Check if proposal extends any existing fork chains
        let index = self.find_extended_chain_index(proposal)?;
        if index == -2 {
            return Err(Error::ExtendedChainIndexNotFound)
        }

        // Validate state transition against canonical state
        // TODO: This should be validated against fork state
        debug!("receive_proposal(): Starting state transition validation");
        if let Err(e) = self.verify_transactions(&proposal.block.txs) {
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

    /// Node checks if any of the fork chains can be finalized.
    /// Consensus finalization logic:
    /// - If the node has observed the creation of 3 proposals in a fork chain and no other
    ///   forks exists at same or greater height, it finalizes (appends to canonical blockchain)
    ///   all proposals up to the last one.
    /// When fork chain proposals are finalized, the rest of fork chains are removed.
    pub async fn chain_finalization(&mut self) -> Result<Vec<BlockInfo>> {
        // First we find longest chain without any other forks at same height
        let mut chain_index = -1;
        let mut max_length = 0;
        for (index, chain) in self.consensus.proposals.iter().enumerate() {
            let length = chain.proposals.len();
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
                debug!("chain_finalization(): Eligible forks with same heigh exist, nothing to finalize");
                return Ok(vec![])
            }
            -1 => {
                debug!("chain_finalization(): All chains have less than 3 proposals, nothing to finalize");
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
        let blockhashes = match self.blockchain.add(&finalized) {
            Ok(v) => v,
            Err(e) => {
                error!("consensus: Failed appending finalized blocks to canonical chain: {}", e);
                return Err(e)
            }
        };

        // Validating state transitions
        for proposal in &finalized {
            // TODO: Is this the right place? We're already doing this in protocol_sync.
            // TODO: These state transitions have already been checked. (I wrote this, but where?)
            // TODO: FIXME: The state transitions have already been written, they have to be in memory
            //              until this point.
            debug!(target: "consensus", "Applying state transition for finalized block");
            if let Err(e) = self.verify_transactions(&proposal.txs) {
                error!(target: "consensus", "Finalized block transaction verifications failed: {}", e);
                return Err(e)
            }
        }

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
            if let Err(e) = self.verify_transactions(&block.txs) {
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

        Ok(())
    }

    /// Validate signatures, wasm execution, and zk proofs for given transactions.
    /// If all of those succeed, try to execute a state update for the contract calls.
    /// Currently the verifications are sequential, and the function will fail if any
    /// of the verifications fail.
    /// TODO: FIXME: TESTNET: The state changes should be in memory until a block with
    ///                       it is finalized. Another option is to not apply and just
    ///                       run this again when we see a finalized block (and apply
    ///                       the update at that point). #finalization
    pub fn verify_transactions(&self, txs: &[Transaction]) -> Result<()> {
        debug!("Verifying {} transaction(s)", txs.len());
        for tx in txs {
            // Table of public inputs used for ZK proof verification
            let mut zkp_table = vec![];
            // Table of public keys used for signature verification
            let mut sig_table = vec![];
            // State updates produced by contract execution
            let mut updates = vec![];

            // Iterate over all calls to get the metadata
            for (idx, call) in tx.calls.iter().enumerate() {
                debug!("Working on call {}", idx);
                // Check if the called contract exist as bincode.
                let bincode = self.blockchain.wasm_bincode.get(call.contract_id)?;
                debug!("Found wasm bincode for {}", call.contract_id);

                // Write the actual payload data
                let mut payload = vec![];
                payload.write_u32(idx as u32)?; // Call index
                tx.calls.encode(&mut payload)?; // Actual call_data

                // Instantiate the wasm runtime
                // TODO: Sum up the gas fees of these calls and instantiations
                let mut runtime =
                    Runtime::new(&bincode, self.blockchain.clone(), call.contract_id)?;

                // Perform the execution to fetch verification metadata
                debug!("Executing \"metadata\" call");
                let metadata = runtime.metadata(&payload)?;
                let mut decoder = Cursor::new(&metadata);
                let zkp_pub: Vec<(String, Vec<pallas::Base>)> = Decodable::decode(&mut decoder)?;
                let sig_pub: Vec<PublicKey> = Decodable::decode(&mut decoder)?;
                // TODO: Make sure we've read all the data above
                zkp_table.push(zkp_pub);
                sig_table.push(sig_pub);
                debug!("Successfully executed \"metadata\" call");

                // Execute the contract call
                debug!("Executing \"exec\" call");
                let update = runtime.exec(&payload)?;
                updates.push(update);
                debug!("Successfully executed \"exec\" call");
            }

            // Verify the Schnorr signatures with the public keys given to us from
            // the metadata call.
            debug!("Verifying transaction signatures");
            tx.verify_sigs(sig_table)?;
            debug!("Signatures verified successfully!");

            // Finally, verify the ZK proofs
            debug!("Verifying transaction ZK proofs");
            // FIXME XXX:
            tx.verify_zkps(&[], zkp_table)?;
            debug!("Transaction ZK proofs verified successfully!");

            // When the verification stage has passed, just apply all the changes.
            // TODO: FIXME: This writes directly to the database. Instead it should live
            //              in memory until things get finalized. (Search #finalization
            //              for additional notes).
            // TODO: We instantiate new runtimes here, so pick up the gas fees from
            //       the previous runs and sum them all together.
            debug!("Performing state updates");
            assert!(tx.calls.len() == updates.len());
            for (call, update) in tx.calls.iter().zip(updates.iter()) {
                // Do the bincode lookups again
                let bincode = self.blockchain.wasm_bincode.get(call.contract_id)?;
                debug!("Found wasm bincode for {}", call.contract_id);

                let mut runtime =
                    Runtime::new(&bincode, self.blockchain.clone(), call.contract_id)?;

                debug!("Executing \"apply\" call");
                runtime.apply(&update)?;
            }
        }

        Ok(())
    }
}
