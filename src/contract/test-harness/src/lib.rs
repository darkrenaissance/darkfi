/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use std::{
    collections::HashMap,
    fs::OpenOptions,
    io::{Cursor, Write},
    slice,
    sync::Arc,
    time::Instant,
};

use darkfi::{
    blockchain::{BlockInfo, Blockchain, BlockchainOverlay},
    runtime::vm_runtime::{Runtime, TxLocalState},
    tx::Transaction,
    util::{
        logger::{setup_test_logger, Level},
        pcg::Pcg32,
        time::Timestamp,
    },
    validator::{utils::deploy_native_contracts, Validator, ValidatorConfig, ValidatorPtr},
    zk::{empty_witnesses, halo2::Field, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_dao_contract::model::{Dao, DaoBulla, DaoProposal, DaoProposalBulla, DaoVoteParams};
use darkfi_money_contract::{
    client::{MoneyNote, OwnCoin},
    model::{
        CoinAttributes, Input, MoneyFeeParamsV1, MoneyGenesisMintParamsV1, Nullifier, Output,
        TokenAttributes, TokenId,
    },
    MoneyFunction,
};
use darkfi_sdk::{
    bridgetree,
    crypto::{
        contract_id::MONEY_CONTRACT_ID,
        poseidon_hash,
        smt::{MemoryStorageFp, PoseidonFp, SmtMemoryFp, EMPTY_NODES_FP},
        BaseBlind, FuncRef, Keypair, MerkleNode, MerkleTree, ScalarBlind, SecretKey,
    },
    pasta::pallas,
};
use darkfi_serial::{serialize, Encodable};
use num_bigint::BigUint;
use parking_lot::Mutex;
use rand::rngs::OsRng;
use sled_overlay::sled;
use tracing::{debug, warn};

/// Utility module for caching ZK proof PKs and VKs
pub mod vks;

/// `Money::Burn` functionality
mod money_burn;
/// `Money::Fee` functionality
mod money_fee;
/// `Money::GenesisMint` functionality
mod money_genesis_mint;
/// `Money::OtcSwap` functionality
mod money_otc_swap;
/// `Money::PoWReward` functionality
mod money_pow_reward;
/// `Money::TokenMint` functionality
mod money_token;
/// `Money::Transfer` functionality
mod money_transfer;

/// `Deployooor::Deploy` functionality
mod contract_deploy;
/// `Deployooor::Lock` functionality
mod contract_lock;

/// `Dao::Exec` functionality
mod dao_exec;
/// `Dao::Mint` functionality
mod dao_mint;
/// `Dao::Propose` functionality
mod dao_propose;
/// `Dao::Vote` functionality
mod dao_vote;

/// PoW target
const POW_TARGET: u32 = 120;

/// Initialize the logging mechanism
pub fn init_logger() {
    // We check this error so we can execute same-file tests in parallel.
    // Otherwise subsequent calls fail to init the logger here.
    if setup_test_logger(
        &["sled"],
        false,
        Level::Info,
        //Level::Verbose,
        //Level::Debug,
        //Level::Trace,
    )
    .is_err()
    {
        warn!(target: "test-harness", "Logger already initialized");
    }
}

/// Enum representing available wallet holders
#[derive(Clone, Copy, Eq, PartialEq, Hash, Debug)]
pub enum Holder {
    Alice,
    Bob,
    Charlie,
    Dao,
    Rachel,
}

/// Wallet instance for a single [`Holder`]
pub struct Wallet {
    /// Main holder keypair
    pub keypair: Keypair,
    /// Keypair for arbitrary token minting
    pub token_mint_authority: Keypair,
    /// Keypair for arbitrary contract deployment
    pub contract_deploy_authority: Keypair,
    /// Holder's [`Validator`] instance
    pub validator: ValidatorPtr,
    /// Holder's instance of the Merkle tree for the `Money` contract
    pub money_merkle_tree: MerkleTree,
    /// Holder's instance of the nullifiers SMT tree for the `Money` contract
    pub money_null_smt: SmtMemoryFp,
    /// Holder's instance of the nullifiers SMT tree for the `Money` contract
    /// snapshotted for `DAO::Propose`
    pub money_null_smt_snapshot: Option<SmtMemoryFp>,
    /// Holder's instance of the Merkle tree for the `DAO` contract
    /// holding DAO bullas
    pub dao_merkle_tree: MerkleTree,
    /// Holder's instance of the Merkle tree for the `DAO` contract
    /// holding DAO proposals
    pub dao_proposals_tree: MerkleTree,
    /// Holder's set of unspent [`OwnCoin`]s from the `Money` contract
    pub unspent_money_coins: Vec<OwnCoin>,
    /// Holder's set of spent [`OwnCoin`]s from the `Money` contract
    pub spent_money_coins: Vec<OwnCoin>,
    /// Witnessed leaf positions of DAO bullas in the `dao_merkle_tree`
    pub dao_leafs: HashMap<DaoBulla, bridgetree::Position>,
    /// Dao Proposal snapshots
    pub dao_prop_leafs: HashMap<DaoProposalBulla, (bridgetree::Position, MerkleTree)>,
    /// Create bench.csv file
    pub bench_wasm: bool,
}

impl Wallet {
    /// Instantiate a new [`Wallet`] instance
    pub async fn new(
        keypair: Keypair,
        token_mint_authority: Keypair,
        contract_deploy_authority: Keypair,
        genesis_block: BlockInfo,
        vks: &vks::Vks,
        verify_fees: bool,
    ) -> Result<Self> {
        // Create an in-memory sled db instance for this wallet
        let sled_db = sled::Config::new().temporary(true).open()?;

        // Inject the cached VKs into the database
        let overlay = BlockchainOverlay::new(&Blockchain::new(&sled_db)?)?;
        vks::inject(&overlay, vks)?;

        deploy_native_contracts(&overlay, POW_TARGET).await?;
        let diff = overlay.lock().unwrap().overlay.lock().unwrap().diff(&[])?;
        overlay.lock().unwrap().contracts.update_state_monotree(&diff)?;
        overlay.lock().unwrap().overlay.lock().unwrap().apply()?;

        // Create the `Validator` instance
        let validator_config = ValidatorConfig {
            confirmation_threshold: 3,
            max_forks: 8,
            pow_target: POW_TARGET,
            pow_fixed_difficulty: Some(BigUint::from(1_u8)),
            genesis_block,
            verify_fees,
        };
        let validator = Validator::new(&sled_db, &validator_config).await?;

        // The Merkle tree for the Money contract is initialized with a
        // "null" leaf at position 0.
        let mut money_merkle_tree = MerkleTree::new(1);
        money_merkle_tree.append(MerkleNode::from(pallas::Base::ZERO));
        money_merkle_tree.mark().unwrap();

        let hasher = PoseidonFp::new();
        let store = MemoryStorageFp::new();
        let money_null_smt = SmtMemoryFp::new(store, hasher, &EMPTY_NODES_FP);

        Ok(Self {
            keypair,
            token_mint_authority,
            contract_deploy_authority,
            validator,
            money_merkle_tree,
            money_null_smt,
            money_null_smt_snapshot: None,
            dao_merkle_tree: MerkleTree::new(1),
            dao_proposals_tree: MerkleTree::new(1),
            unspent_money_coins: vec![],
            spent_money_coins: vec![],
            dao_leafs: HashMap::new(),
            dao_prop_leafs: HashMap::new(),
            bench_wasm: false,
        })
    }

    pub async fn add_transaction(
        &mut self,
        callname: &str,
        tx: Transaction,
        block_height: u32,
    ) -> Result<()> {
        if self.bench_wasm {
            let _ = benchmark_wasm_calls(callname, self.validator.clone(), &tx, block_height).await;
        }

        let validator = self.validator.read().await;
        validator
            .add_test_transactions(
                slice::from_ref(&tx),
                block_height,
                validator.consensus.module.target,
                true,
                validator.verify_fees,
            )
            .await?;

        // Write the data
        let blockchain = &validator.blockchain;
        let txs = &blockchain.transactions;
        txs.insert(slice::from_ref(&tx)).expect("insert tx");
        txs.insert_location(&[tx.hash()], block_height).expect("insert loc");

        Ok(())
    }

    /// Mark a single nullifier as spent in the SMT and move any matching
    /// OwnCoin from `unspent_money_coins` to `spent_money_coins`
    pub fn mark_spent_nullifier(&mut self, nullifier: Nullifier, holder: &Holder) {
        let n = nullifier.inner();
        self.money_null_smt.insert_batch(vec![(n, n)]).expect("smt.insert_batch()");

        if let Some(spent_coin) =
            self.unspent_money_coins.iter().find(|x| x.nullifier() == nullifier).cloned()
        {
            debug!("Found spent OwnCoin({}) for {:?}", spent_coin.coin, holder);
            self.unspent_money_coins.retain(|x| x.nullifier() != nullifier);
            self.spent_money_coins.push(spent_coin);
        }
    }

    /// Process a set of [`Input`]s.
    /// Insert nullifiers into the SMT and move any matching OwnCoins from
    /// unspent to spent.
    pub fn process_inputs(&mut self, inputs: &[Input], holder: &Holder) {
        for input in inputs {
            self.mark_spent_nullifier(input.nullifier, holder);
        }
    }

    /// Process a set of [`Output`]s.
    /// Append each coin to the Merkle tree and attempt to decrypt the note.
    /// Returns any new OwnCoins found.
    pub fn process_outputs(&mut self, outputs: &[Output], holder: &Holder) -> Vec<OwnCoin> {
        let mut found = vec![];

        for output in outputs {
            self.money_merkle_tree.append(MerkleNode::from(output.coin.inner()));

            let Ok(note) = output.note.decrypt::<MoneyNote>(&self.keypair.secret) else { continue };

            let owncoin = OwnCoin {
                coin: output.coin,
                note: note.clone(),
                secret: self.keypair.secret,
                leaf_position: self.money_merkle_tree.mark().unwrap(),
            };

            debug!("Found new OwnCoin({}) for {:?}", owncoin.coin, holder);
            self.unspent_money_coins.push(owncoin.clone());
            found.push(owncoin);
        }

        found
    }

    /// Process the fee component of a transaction (if present).
    /// Handles the fee input nullifier and fee change output.
    /// Returns any new OwnCoins found.
    pub fn process_fee(
        &mut self,
        fee_params: &Option<MoneyFeeParamsV1>,
        holder: &Holder,
    ) -> Vec<OwnCoin> {
        let Some(ref fp) = fee_params else { return vec![] };

        self.mark_spent_nullifier(fp.input.nullifier, holder);
        self.process_outputs(slice::from_ref(&fp.output), holder)
    }
}

/// Native contract test harness instance
pub struct TestHarness {
    /// Initialized [`Holder`]s for this instance
    pub holders: HashMap<Holder, Wallet>,
    /// Ordered list of all holder keys (for broadcast operations)
    pub holder_keys: Vec<Holder>,
    /// Cached [`ProvingKey`]s for native contract ZK proving
    pub proving_keys: HashMap<String, (ProvingKey, ZkBinary)>,
    /// The genesis block for this harness
    pub genesis_block: BlockInfo,
    /// Marker to know if we're supposed to include tx fees
    pub verify_fees: bool,
}

impl TestHarness {
    /// Instantiate a new [`TestHarness`] given a slice of [`Holder`]s.
    /// Additionally, a `verify_fees` boolean will enforce tx fee verification.
    pub async fn new(holders: &[Holder], verify_fees: bool) -> Result<Self> {
        // Create a genesis block
        let mut genesis_block = BlockInfo::default();
        genesis_block.header.timestamp = Timestamp::from_u64(1689772567);
        let producer_tx = genesis_block.txs.pop().unwrap();
        genesis_block.append_txs(vec![producer_tx]);

        // Deterministic PRNG
        let mut rng = Pcg32::new(42);

        // Build or read cached ZK PKs and VKs
        let (pks, vks) = vks::get_cached_pks_and_vks()?;
        let mut proving_keys = HashMap::new();
        for (bincode, namespace, pk) in pks {
            let mut reader = Cursor::new(pk);
            let zkbin = ZkBinary::decode(&bincode, false)?;
            let circuit = ZkCircuit::new(empty_witnesses(&zkbin)?, &zkbin);
            let proving_key = ProvingKey::read(&mut reader, circuit)?;
            proving_keys.insert(namespace, (proving_key, zkbin));
        }

        // Compute genesis contracts states monotree root
        let sled_db = sled::Config::new().temporary(true).open()?;
        let overlay = BlockchainOverlay::new(&Blockchain::new(&sled_db)?)?;
        vks::inject(&overlay, &vks)?;
        deploy_native_contracts(&overlay, POW_TARGET).await?;
        let diff = overlay.lock().unwrap().overlay.lock().unwrap().diff(&[])?;
        genesis_block.header.state_root =
            overlay.lock().unwrap().contracts.update_state_monotree(&diff)?;

        // Create `Wallet` instances
        let mut holders_map = HashMap::new();
        let mut holder_keys = Vec::with_capacity(holders.len());

        for holder in holders {
            let keypair = Keypair::random(&mut rng);
            let token_mint_authority = Keypair::random(&mut rng);
            let contract_deploy_authority = Keypair::random(&mut rng);

            let wallet = Wallet::new(
                keypair,
                token_mint_authority,
                contract_deploy_authority,
                genesis_block.clone(),
                &vks,
                verify_fees,
            )
            .await?;

            holders_map.insert(*holder, wallet);
            holder_keys.push(*holder);
        }

        Ok(Self { holders: holders_map, holder_keys, proving_keys, genesis_block, verify_fees })
    }

    /// Get a reference to a Holder's Wallet
    pub fn wallet(&self, holder: &Holder) -> &Wallet {
        self.holders.get(holder).unwrap()
    }

    /// Get a mutable reference to a Holder's Wallet
    pub fn wallet_mut(&mut self, holder: &Holder) -> &mut Wallet {
        self.holders.get_mut(holder).unwrap()
    }

    /// Get a Holder's unspent OwnCoins
    pub fn coins(&self, holder: &Holder) -> &[OwnCoin] {
        &self.wallet(holder).unspent_money_coins
    }

    /// Get a Holder's unspent OwnCoins filtered by token ID
    pub fn coins_by_token(&self, holder: &Holder, token_id: TokenId) -> Vec<OwnCoin> {
        self.coins(holder).iter().filter(|c| c.note.token_id == token_id).cloned().collect()
    }

    /// Get the total balance of a Holder for a given token
    pub fn balance(&self, holder: &Holder, token_id: TokenId) -> u64 {
        self.coins(holder)
            .iter()
            .filter(|c| c.note.token_id == token_id)
            .map(|c| c.note.value)
            .sum()
    }

    /// Assert that all holders' Merkle trees are consistent
    pub fn assert_trees(&self, holders: &[Holder]) {
        assert!(!holders.is_empty());
        let mut wallets = vec![];
        for holder in holders {
            wallets.push(self.holders.get(holder).unwrap());
        }

        let money_root = wallets[0].money_merkle_tree.root(0).unwrap();
        for wallet in &wallets[1..] {
            assert_eq!(money_root, wallet.money_merkle_tree.root(0).unwrap());
        }
    }

    /// Assert all registered holders' Merkle trees are consistent
    pub fn assert_all_trees(&self) {
        if !self.holder_keys.is_empty() {
            self.assert_trees(&self.holder_keys);
        }
    }

    /// Mint a token for `recipient` and execute the tx on all registered
    /// holders.
    /// Returns the minted `TokenId`
    pub async fn token_mint_to_all(
        &mut self,
        amount: u64,
        holder: &Holder,
        recipient: &Holder,
        block_height: u32,
    ) -> Result<TokenId> {
        let token_blind = BaseBlind::random(&mut OsRng);
        let (tx, mint_params, auth_params, fee_params) = self
            .token_mint(amount, holder, recipient, token_blind, None, None, block_height)
            .await?;

        // Derive the Token ID
        let token_id = self.derive_token_id(recipient, token_blind);

        let holders = self.holder_keys.clone();
        for h in &holders {
            self.execute_token_mint_tx(
                h,
                tx.clone(),
                &mint_params,
                &auth_params,
                &fee_params,
                block_height,
                true,
            )
            .await?;
        }

        self.assert_all_trees();

        Ok(token_id)
    }

    /// Mint a token with a specific `token_blind` and execute on all
    /// registered holders. Returns the [`TokenId`].
    ///
    /// Use this instead of `token_mint_to_all` when you need the same
    /// token blind across multiple mints (e.g. DAO governance tokens).
    pub async fn token_mint_with_blind_to_all(
        &mut self,
        amount: u64,
        holder: &Holder,
        recipient: &Holder,
        token_blind: BaseBlind,
        block_height: u32,
    ) -> Result<TokenId> {
        let (tx, mint_params, auth_params, fee_params) = self
            .token_mint(amount, holder, recipient, token_blind, None, None, block_height)
            .await?;

        let token_id = self.derive_token_id(recipient, token_blind);

        let holders = self.holder_keys.clone();
        for h in &holders {
            self.execute_token_mint_tx(
                h,
                tx.clone(),
                &mint_params,
                &auth_params,
                &fee_params,
                block_height,
                true,
            )
            .await?;
        }
        self.assert_all_trees();

        Ok(token_id)
    }

    /// Transfer `amount` of `token_id` from `sender` to `recipient` and
    /// execute the tx on all registered holders.
    pub async fn transfer_to_all(
        &mut self,
        amount: u64,
        sender: &Holder,
        recipient: &Holder,
        token_id: TokenId,
        block_height: u32,
    ) -> Result<()> {
        let owncoins = self.coins_by_token(sender, token_id);
        let (tx, (params, fee_params), _spent) = self
            .transfer(amount, sender, recipient, &owncoins, token_id, block_height, false)
            .await?;

        let holders = self.holder_keys.clone();
        for h in &holders {
            self.execute_transfer_tx(h, tx.clone(), &params, &fee_params, block_height, true)
                .await?;
        }

        self.assert_all_trees();

        Ok(())
    }

    /// Burn given [`OwnCoin`]s and execute the tx on all registered holders.
    pub async fn burn_to_all(
        &mut self,
        holder: &Holder,
        coins: &[OwnCoin],
        block_height: u32,
    ) -> Result<()> {
        let (tx, (params, fee_params), _spent) = self.burn(holder, coins, block_height).await?;

        let holders = self.holder_keys.clone();
        for h in &holders {
            self.execute_burn_tx(h, tx.clone(), &params, &fee_params, block_height, true).await?;
        }

        self.assert_all_trees();

        Ok(())
    }

    /// Build a genesis mint for `holder` and execute on all registered holders.
    /// Returns the found [`OwnCoin`]s.
    pub async fn genesis_mint_to_all(
        &mut self,
        holder: &Holder,
        amounts: &[u64],
        block_height: u32,
    ) -> Result<Vec<OwnCoin>> {
        let (tx, params) = self.genesis_mint(holder, amounts, None, None).await?;
        self.genesis_mint_to_all_with(tx, &params, block_height).await
    }

    /// Execute a pre-built genesis mint transaction on all registered holders.
    /// Useful when you need to test the transaction before broadcasting
    /// (e.g. malicious block height checks).
    pub async fn genesis_mint_to_all_with(
        &mut self,
        tx: Transaction,
        params: &MoneyGenesisMintParamsV1,
        block_height: u32,
    ) -> Result<Vec<OwnCoin>> {
        let holders = self.holder_keys.clone();
        let mut found = vec![];
        for h in &holders {
            found.extend(
                self.execute_genesis_mint_tx(h, tx.clone(), params, block_height, true).await?,
            );
        }
        self.assert_all_trees();
        Ok(found)
    }

    /// Freeze a token authority for `holder` and execute on all registered
    /// holders.
    pub async fn token_freeze_to_all(&mut self, holder: &Holder, block_height: u32) -> Result<()> {
        let (tx, freeze_params, fee_params) = self.token_freeze(holder, block_height).await?;

        let holders = self.holder_keys.clone();
        for h in &holders {
            self.execute_token_freeze_tx(
                h,
                tx.clone(),
                &freeze_params,
                &fee_params,
                block_height,
                true,
            )
            .await?;
        }
        self.assert_all_trees();
        Ok(())
    }

    /// Generate a new block mined by `miner` and broadcast to all registered
    /// holders. Convenience wrapper around `generate_block` that uses
    /// `holder_keys` instead of requiring the caller to pass holders.
    pub async fn generate_block_all(&mut self, miner: &Holder) -> Result<Vec<OwnCoin>> {
        let holders = self.holder_keys.clone();
        self.generate_block(miner, &holders).await
    }

    /// Consolidate all coins of `token_id` owned by `holder` into a single
    /// coin by transferring to self, then execute on all registered holders.
    pub async fn consolidate_to_all(
        &mut self,
        holder: &Holder,
        token_id: TokenId,
        block_height: u32,
    ) -> Result<()> {
        let owncoins = self.coins_by_token(holder, token_id);
        if owncoins.len() <= 1 {
            // Nothing to consolidate
            return Ok(())
        }

        let total: u64 = owncoins.iter().map(|c| c.note.value).sum();
        let (tx, (params, fee_params), _spent) =
            self.transfer(total, holder, holder, &owncoins, token_id, block_height, false).await?;

        let holders = self.holder_keys.clone();
        for h in &holders {
            self.execute_transfer_tx(h, tx.clone(), &params, &fee_params, block_height, true)
                .await?;
        }

        self.assert_all_trees();

        Ok(())
    }

    /// Perform an OTC swap between two holders and execute on all registered
    /// holders.
    pub async fn otc_swap_to_all(
        &mut self,
        holder0: &Holder,
        coin0: &OwnCoin,
        holder1: &Holder,
        coin1: &OwnCoin,
        block_height: u32,
    ) -> Result<()> {
        let (tx, params, fee_params) =
            self.otc_swap(holder0, coin0, holder1, coin1, block_height).await?;

        let holders = self.holder_keys.clone();
        for h in &holders {
            self.execute_otc_swap_tx(h, tx.clone(), &params, &fee_params, block_height, true)
                .await?;
        }

        self.assert_all_trees();

        Ok(())
    }

    /// Build a `Dao::Mint` transaction and execute on all registered holders.
    #[allow(clippy::too_many_arguments)]
    pub async fn dao_mint_to_all(
        &mut self,
        holder: &Holder,
        dao: &Dao,
        dao_notes_secret_key: &SecretKey,
        dao_proposer_secret_key: &SecretKey,
        dao_proposals_secret_key: &SecretKey,
        dao_votes_secret_key: &SecretKey,
        dao_exec_secret_key: &SecretKey,
        dao_early_exec_secret_key: &SecretKey,
        block_height: u32,
    ) -> Result<()> {
        let (tx, params, fee_params) = self
            .dao_mint(
                holder,
                dao,
                dao_notes_secret_key,
                dao_proposer_secret_key,
                dao_proposals_secret_key,
                dao_votes_secret_key,
                dao_exec_secret_key,
                dao_early_exec_secret_key,
                block_height,
            )
            .await?;

        let holders = self.holder_keys.clone();
        for h in &holders {
            self.execute_dao_mint_tx(h, tx.clone(), &params, &fee_params, block_height, true)
                .await?;
        }
        self.assert_all_trees();
        Ok(())
    }

    /// Build a `Dao::Propose` (transfer) transaction and execute on all
    /// registered holders. Returns the [`DaoProposal`] for subsequent
    /// voting/execution.
    #[allow(clippy::too_many_arguments)]
    pub async fn dao_propose_transfer_to_all(
        &mut self,
        proposer: &Holder,
        proposal_coinattrs: &[CoinAttributes],
        user_data: pallas::Base,
        dao: &Dao,
        dao_proposer_secret_key: &SecretKey,
        block_height: u32,
        duration_blockwindows: u64,
    ) -> Result<DaoProposal> {
        let (tx, params, fee_params, proposal_info) = self
            .dao_propose_transfer(
                proposer,
                proposal_coinattrs,
                user_data,
                dao,
                dao_proposer_secret_key,
                block_height,
                duration_blockwindows,
            )
            .await?;

        let holders = self.holder_keys.clone();
        for h in &holders {
            self.execute_dao_propose_tx(h, tx.clone(), &params, &fee_params, block_height, true)
                .await?;
        }
        self.assert_all_trees();
        Ok(proposal_info)
    }

    /// Build a `Dao::Propose` (generic) transaction and execute on all
    /// registered holders. Returns the [`DaoProposal`].
    pub async fn dao_propose_generic_to_all(
        &mut self,
        proposer: &Holder,
        user_data: pallas::Base,
        dao: &Dao,
        dao_proposer_secret_key: &SecretKey,
        block_height: u32,
        duration_blockwindows: u64,
    ) -> Result<DaoProposal> {
        let (tx, params, fee_params, proposal_info) = self
            .dao_propose_generic(
                proposer,
                user_data,
                dao,
                dao_proposer_secret_key,
                block_height,
                duration_blockwindows,
            )
            .await?;

        let holders = self.holder_keys.clone();
        for h in &holders {
            self.execute_dao_propose_tx(h, tx.clone(), &params, &fee_params, block_height, true)
                .await?;
        }
        self.assert_all_trees();
        Ok(proposal_info)
    }

    /// Build and broadcast a single `Dao::Vote` transaction on all registered
    /// holders. Returns the [`DaoVoteParams`] (needed for vote counting).
    pub async fn dao_vote_to_all(
        &mut self,
        voter: &Holder,
        vote_option: bool,
        dao: &Dao,
        proposal: &DaoProposal,
        block_height: u32,
    ) -> Result<DaoVoteParams> {
        let (tx, vote_params, fee_params) =
            self.dao_vote(voter, vote_option, dao, proposal, block_height).await?;

        let holders = self.holder_keys.clone();
        for h in &holders {
            self.execute_dao_vote_tx(h, tx.clone(), &fee_params, block_height, true).await?;
        }
        self.assert_all_trees();
        Ok(vote_params)
    }

    /// Build and broadcast a `Dao::Exec` (transfer) transaction on all
    /// registered holders.
    #[allow(clippy::too_many_arguments)]
    pub async fn dao_exec_transfer_to_all(
        &mut self,
        executor: &Holder,
        dao: &Dao,
        dao_exec_secret_key: &SecretKey,
        dao_early_exec_secret_key: &Option<SecretKey>,
        proposal_info: &DaoProposal,
        proposal_coinattrs: Vec<CoinAttributes>,
        total_yes_vote_value: u64,
        total_all_vote_value: u64,
        total_yes_vote_blind: ScalarBlind,
        total_all_vote_blind: ScalarBlind,
        block_height: u32,
    ) -> Result<()> {
        let (tx, xfer_params, fee_params) = self
            .dao_exec_transfer(
                executor,
                dao,
                dao_exec_secret_key,
                dao_early_exec_secret_key,
                proposal_info,
                proposal_coinattrs,
                total_yes_vote_value,
                total_all_vote_value,
                total_yes_vote_blind,
                total_all_vote_blind,
                block_height,
            )
            .await?;

        let holders = self.holder_keys.clone();
        for h in &holders {
            self.execute_dao_exec_tx(
                h,
                tx.clone(),
                Some(&xfer_params),
                &fee_params,
                block_height,
                true,
            )
            .await?;
        }
        self.assert_all_trees();
        Ok(())
    }

    /// Build and broadcast a `Dao::Exec` (generic) transaction on all
    /// registered holders.
    #[allow(clippy::too_many_arguments)]
    pub async fn dao_exec_generic_to_all(
        &mut self,
        executor: &Holder,
        dao: &Dao,
        dao_exec_secret_key: &SecretKey,
        dao_early_exec_secret_key: &Option<SecretKey>,
        proposal_info: &DaoProposal,
        total_yes_vote_value: u64,
        total_all_vote_value: u64,
        total_yes_vote_blind: ScalarBlind,
        total_all_vote_blind: ScalarBlind,
        block_height: u32,
    ) -> Result<()> {
        let (tx, fee_params) = self
            .dao_exec_generic(
                executor,
                dao,
                dao_exec_secret_key,
                dao_early_exec_secret_key,
                proposal_info,
                total_yes_vote_value,
                total_all_vote_value,
                total_yes_vote_blind,
                total_all_vote_blind,
                block_height,
            )
            .await?;

        let holders = self.holder_keys.clone();
        for h in &holders {
            self.execute_dao_exec_tx(h, tx.clone(), None, &fee_params, block_height, true).await?;
        }
        self.assert_all_trees();
        Ok(())
    }

    /// Derive the [`TokenId`] that a given holder's `token_mint_authority`
    /// would produce with a given blind.
    pub fn derive_token_id(&self, holder: &Holder, token_blind: BaseBlind) -> TokenId {
        let wallet = self.wallet(holder);
        let mint_authority = wallet.token_mint_authority;

        let auth_func_id = FuncRef {
            contract_id: *MONEY_CONTRACT_ID,
            func_code: MoneyFunction::AuthTokenMintV1 as u8,
        }
        .to_func_id();

        let token_attrs = TokenAttributes {
            auth_parent: auth_func_id,
            user_data: poseidon_hash([mint_authority.public.x(), mint_authority.public.y()]),
            blind: token_blind,
        };

        token_attrs.to_token_id()
    }
}

async fn benchmark_wasm_calls(
    callname: &str,
    validator: ValidatorPtr,
    tx: &Transaction,
    block_height: u32,
) -> Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open("bench.csv")?;

    let tx_local_state = Arc::new(Mutex::new(TxLocalState::new()));

    let validator = validator.read().await;
    for (idx, call) in tx.calls.iter().enumerate() {
        let overlay = BlockchainOverlay::new(&validator.blockchain).expect("blockchain overlay");
        let wasm = overlay.lock().unwrap().contracts.get(call.data.contract_id)?;

        tx_local_state.lock().entry(call.data.contract_id).or_default();

        let mut runtime = Runtime::new(
            &wasm,
            overlay.clone(),
            tx_local_state.clone(),
            call.data.contract_id,
            block_height,
            validator.consensus.module.target,
            tx.hash(),
            idx as u8,
        )
        .expect("runtime");

        // Write call data
        let mut payload = vec![];
        tx.calls.encode(&mut payload)?;

        let mut times = [0; 3];
        let now = Instant::now();
        let _metadata = runtime.metadata(&payload)?;
        times[0] = now.elapsed().as_micros();

        let now = Instant::now();
        let mut update = vec![call.data.data[0]];
        update.append(&mut runtime.exec(&payload)?);
        times[1] = now.elapsed().as_micros();

        let now = Instant::now();
        runtime.apply(&update)?;
        times[2] = now.elapsed().as_micros();

        writeln!(
            file,
            "{},{},{},{},{},{},{}",
            callname,
            tx.hash(),
            idx,
            times[0],
            times[1],
            times[2],
            serialize(tx).len(),
        )?;
    }

    Ok(())
}
