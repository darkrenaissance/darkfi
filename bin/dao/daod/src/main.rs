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

use std::{collections::HashMap, sync::Arc, time::Instant};

use log::debug;
use rand::rngs::OsRng;
use simplelog::{ColorChoice, LevelFilter, TermLogger, TerminalMode};
use url::Url;

use darkfi::{
    crypto::{
        proof::{ProvingKey, VerifyingKey},
        types::{DrkSpendHook, DrkUserData, DrkValue},
    },
    rpc::server::listen_and_serve,
    zk::circuit::{BurnContract, MintContract},
    zkas::ZkBinary,
    Error, Result,
};
use darkfi_sdk::crypto::{
    pedersen::pedersen_commitment_u64, poseidon_hash, Keypair, MerkleNode, PublicKey, SecretKey,
    TokenId,
};
use incrementalmerkletree::{Position, Tree};
use pasta_curves::{
    group::{ff::Field, Group},
    pallas,
};

mod contract;
mod error;
mod note;
mod rpc;
mod util;

use crate::{
    contract::{
        dao::{self, mint::wallet::DaoParams, propose::wallet::Proposal, DaoBulla},
        money::{self, state::OwnCoin},
    },
    error::{DaoError, DaoResult},
    rpc::JsonRpcInterface,
    util::{sign, StateRegistry, Transaction, ZkContractTable, DRK_ID, GOV_ID},
};

//////////////////////////////////////////////////////////////////////////
//////////////////////////////////////////////////////////////////////////
//// dao-demo 0.1
////
//// This is a very early prototype intended to demonstrate the underlying
//// crypto of fully anonymous DAOs. DAO participants can own and operate
//// a collective treasury according to rules set by the DAO. Communities
//// can coordinate financially in the cover of a protective darkness,
//// free from surveillance and persecution.
////
//// The following information is completely hidden:
////
//// * DAO treasury
//// * DAO parameters
//// * DAO participants
//// * Proposals
//// * Votes
////
//// The DAO enables participants to make proposals, cast votes, and spend
//// money from the DAO treasury if a proposal passes. The basic operation
//// involves transferring money from a treasury to a public key specified
//// in a Proposal. This operation can only happen if several conditions are
//// met.
////
//// At its basis, the DAO is a treasury that is owned by everyone who holds
//// the DAO governance token. These constraints, also known as DAO parameters,
//// are configured by DAO participants and enforced by ZK cryptography.
////
//// In this demo, the constraints are:
////
//// 1. DAO quorum: the number of governance tokens that must be allocated
////    to a proposal in order for a proposal to pass.
//// 2. Proposer limit: the number of governance tokens required to make a
////    proposal.
//// 3. DAO approval ratio: The ratio of yes/ no votes required for a
////    proposal to pass.
////
//// In addition, DAO participants must prove ownership of governance tokens
//// order to vote. Their vote is weighted according to the number of governance
//// tokens in their wallet. In this current implementation, users do not spend
//// or lock up these coins in order to vote- they simply prove ownership of them.
////
//// In the current prototype, the following information is exposed:
////
//// * Encrypted votes are publicly linked to the proposal identifier hash,
////   meaning that it is possible to see that there is voting activity associated
////   with a particular proposal identifier, but the contents of the proposal,
////   how one has voted, and the associated DAO is fully private.
//// * In the burn phase of casting a vote, we reveal a public value called a
////   nullifier. The same public value is revealed when we spend the coins we
////   used to vote, meaning you can link a vote with a user when they spend
////   governance tokens. This is bad but is easily fixable. We will update the
////   code to use different values in the vote (by creating an intermediate Coin
////   used for voting).
//// * Votes are currently encrypted to the DAO public key. This means that
////   any DAO participant can decrypt votes as they come in. In the future,
////   we can delay the decryption so that you cannot read votes until the final
////   tally.
////
//// Additionally, the dao-demo app shown below is highly limited. Namely, we use
//// a single God daemon to operate all the wallets. In the next version, every user
//// wallet will be a seperate daemon connecting over a network and running on a
//// blockchain.
////
//// /////////////////////////////////////////////////////////////////////
////
//// dao-demo 0.1 TODOs:
////
//// High priority:
////
//// 5. vote() should pass a ProposalBulla
////
//// Less priority:
////
//// 1. Better document CLI/ CLI help.
////
//// 2. Token id is hardcoded rn. Change this so users can specify token_id
////    as either xdrk or gdrk. In dao-cli we run a match statement to link to
////    the corresponding static values XDRK_ID and GDRK_ID. Note: xdrk is used
////    only for the DAO treasury. gdrk is the governance token used to operate
////    the DAO.
////
//// 3. Implement money transfer between MoneyWallets so users can send tokens to
////    eachother.
////
//// 4. Make CLI usage more interactive. Example: when I cast a vote, output:
////   "You voted {} with value {}." where value is the number of gDRK in a users
////    wallet (and the same for making a proposal etc).
////
//// 5. Currently, DaoWallet stores DaoParams, DaoBulla's and Proposal's in a
////    Vector. We retrieve values through indexing, meaning that we
////    cannot currently support multiple DAOs and multiple proposals.
////
////    Instead, dao_wallet.create_dao() should create a struct called Dao
////    which stores dao_info: HashMap<DaoBulla, DaoParams> and proposals:
////    HashMap<ProposalBulla, Proposal>. Users pass the DaoBulla and
////    ProposalBulla and we lookup the corresponding data. struct Dao should
////    be owned by DaoWallet.
////
//// 6. Error handling :)
////
//////////////////////////////////////////////////////////////////////////
//////////////////////////////////////////////////////////////////////////

pub struct Client {
    dao_wallet: DaoWallet,
    money_wallets: HashMap<[u8; 32], MoneyWallet>,
    cashier_wallet: CashierWallet,
    states: StateRegistry,
    zk_bins: ZkContractTable,
}

impl Client {
    fn new() -> Self {
        // For this early demo we store all wallets in a single Client.
        let dao_wallet = DaoWallet::new();
        let money_wallets = HashMap::new();
        let cashier_wallet = CashierWallet::new();

        // Lookup table for smart contract states
        let states = StateRegistry::new();

        // Initialize ZK binary table
        let zk_bins = ZkContractTable::new();

        Self { dao_wallet, money_wallets, cashier_wallet, states, zk_bins }
    }

    // Load ZK contracts into the ZkContractTable and initialize the StateRegistry.
    fn init(&mut self) -> Result<()> {
        //We use these to initialize the money state.
        let faucet_signature_secret = SecretKey::random(&mut OsRng);
        let faucet_signature_public = PublicKey::from_secret(faucet_signature_secret);

        debug!(target: "demo", "Loading dao-mint.zk");
        let zk_dao_mint_bincode = include_bytes!("../proof/dao-mint.zk.bin");
        let zk_dao_mint_bin = ZkBinary::decode(zk_dao_mint_bincode)?;
        self.zk_bins.add_contract("dao-mint".to_string(), zk_dao_mint_bin, 13);

        debug!(target: "demo", "Loading money-transfer contracts");
        let start = Instant::now();
        let mint_pk = ProvingKey::build(11, &MintContract::default());
        debug!("Mint PK: [{:?}]", start.elapsed());
        let start = Instant::now();
        let burn_pk = ProvingKey::build(11, &BurnContract::default());
        debug!("Burn PK: [{:?}]", start.elapsed());
        let start = Instant::now();
        let mint_vk = VerifyingKey::build(11, &MintContract::default());
        debug!("Mint VK: [{:?}]", start.elapsed());
        let start = Instant::now();
        let burn_vk = VerifyingKey::build(11, &BurnContract::default());
        debug!("Burn VK: [{:?}]", start.elapsed());

        self.zk_bins.add_native("money-transfer-mint".to_string(), mint_pk, mint_vk);
        self.zk_bins.add_native("money-transfer-burn".to_string(), burn_pk, burn_vk);
        debug!(target: "demo", "Loading dao-propose-main.zk");
        let zk_dao_propose_main_bincode = include_bytes!("../proof/dao-propose-main.zk.bin");
        let zk_dao_propose_main_bin = ZkBinary::decode(zk_dao_propose_main_bincode)?;
        self.zk_bins.add_contract("dao-propose-main".to_string(), zk_dao_propose_main_bin, 13);
        debug!(target: "demo", "Loading dao-propose-burn.zk");
        let zk_dao_propose_burn_bincode = include_bytes!("../proof/dao-propose-burn.zk.bin");
        let zk_dao_propose_burn_bin = ZkBinary::decode(zk_dao_propose_burn_bincode)?;
        self.zk_bins.add_contract("dao-propose-burn".to_string(), zk_dao_propose_burn_bin, 13);
        debug!(target: "demo", "Loading dao-vote-main.zk");
        let zk_dao_vote_main_bincode = include_bytes!("../proof/dao-vote-main.zk.bin");
        let zk_dao_vote_main_bin = ZkBinary::decode(zk_dao_vote_main_bincode)?;
        self.zk_bins.add_contract("dao-vote-main".to_string(), zk_dao_vote_main_bin, 13);
        debug!(target: "demo", "Loading dao-vote-burn.zk");
        let zk_dao_vote_burn_bincode = include_bytes!("../proof/dao-vote-burn.zk.bin");
        let zk_dao_vote_burn_bin = ZkBinary::decode(zk_dao_vote_burn_bincode)?;
        self.zk_bins.add_contract("dao-vote-burn".to_string(), zk_dao_vote_burn_bin, 13);
        let zk_dao_exec_bincode = include_bytes!("../proof/dao-exec.zk.bin");
        let zk_dao_exec_bin = ZkBinary::decode(zk_dao_exec_bincode)?;
        self.zk_bins.add_contract("dao-exec".to_string(), zk_dao_exec_bin, 13);

        let cashier_signature_public = self.cashier_wallet.signature_public();

        let money_state =
            money::state::State::new(cashier_signature_public, faucet_signature_public);
        self.states.register(*money::CONTRACT_ID, money_state);

        let dao_state = dao::State::new();
        self.states.register(*dao::CONTRACT_ID, dao_state);

        Ok(())
    }

    fn create_dao(
        &mut self,
        dao_proposer_limit: u64,
        dao_quorum: u64,
        dao_approval_ratio_quot: u64,
        dao_approval_ratio_base: u64,
        token_id: TokenId,
    ) -> DaoResult<pallas::Base> {
        let tx = self.dao_wallet.mint_tx(
            dao_proposer_limit,
            dao_quorum,
            dao_approval_ratio_quot,
            dao_approval_ratio_base,
            token_id,
            &self.zk_bins,
        );

        self.validate(&tx)?;
        // Only witness the value once the transaction is confirmed.
        self.dao_wallet.update_witness(&mut self.states)?;

        // Retrieve DAO bulla from the state.
        let dao_bulla = {
            let func_call = &tx.func_calls[0];
            let call_data = func_call.call_data.as_any();
            let call_data = call_data.downcast_ref::<dao::mint::validate::CallData>().unwrap();
            call_data.dao_bulla.clone()
        };

        debug!(target: "demo", "Create DAO bulla: {:?}", dao_bulla.0);

        // We store these values in a vector we can easily retrieve DAO values for the demo.
        let dao_params = DaoParams {
            proposer_limit: dao_proposer_limit,
            quorum: dao_quorum,
            approval_ratio_quot: dao_approval_ratio_quot,
            approval_ratio_base: dao_approval_ratio_base,
            gov_token_id: token_id,
            public_key: self.dao_wallet.keypair.public,
            bulla_blind: self.dao_wallet.bulla_blind,
        };

        self.dao_wallet.params.push(dao_params);
        self.dao_wallet.bullas.push(dao_bulla.clone());

        Ok(dao_bulla.0)
    }

    fn mint_treasury(
        &mut self,
        token_id: TokenId,
        token_supply: u64,
        recipient: PublicKey,
    ) -> DaoResult<()> {
        self.dao_wallet.track(&mut self.states)?;

        let tx = self.cashier_wallet.mint(
            token_id,
            token_supply,
            self.dao_wallet.bullas[0].0,
            recipient,
            &self.zk_bins,
        )?;

        self.validate(&tx)?;
        self.update_wallets()?;

        Ok(())
    }

    fn airdrop_user(&mut self, value: u64, token_id: TokenId, addr: PublicKey) -> DaoResult<()> {
        // let wallet = self.money_wallets.get(&nym).unwrap();
        // let addr = wallet.get_public_key();

        let tx = self.cashier_wallet.airdrop(value, token_id, addr, &self.zk_bins)?;
        self.validate(&tx)?;
        self.update_wallets()?;

        Ok(())
    }

    fn validate(&mut self, tx: &Transaction) -> DaoResult<()> {
        debug!(target: "dao_demo::client::validate()", "commencing validate sequence");
        let mut updates = vec![];

        // Validate all function calls in the tx
        for (idx, func_call) in tx.func_calls.iter().enumerate() {
            // So then the verifier will lookup the corresponding state_transition and apply
            // functions based off the func_id

            if func_call.func_id == *money::transfer::FUNC_ID {
                debug!("money_contract::transfer::state_transition()");
                match money::transfer::validate::state_transition(&self.states, idx, tx) {
                    Ok(update) => {
                        updates.push(update);
                    }
                    Err(e) => return Err(DaoError::StateTransitionFailed(e.to_string())),
                }
            } else if func_call.func_id == *dao::mint::FUNC_ID {
                debug!("dao_contract::mint::state_transition()");
                match dao::mint::validate::state_transition(&self.states, idx, tx) {
                    Ok(update) => {
                        updates.push(update);
                    }
                    Err(e) => return Err(DaoError::StateTransitionFailed(e.to_string())),
                }
            } else if func_call.func_id == *dao::propose::FUNC_ID {
                debug!(target: "demo", "dao_contract::propose::state_transition()");
                match dao::propose::validate::state_transition(&self.states, idx, tx) {
                    Ok(update) => {
                        updates.push(update);
                    }
                    Err(e) => return Err(DaoError::StateTransitionFailed(e.to_string())),
                }
            } else if func_call.func_id == *dao::vote::FUNC_ID {
                debug!(target: "demo", "dao_contract::vote::state_transition()");
                match dao::vote::validate::state_transition(&self.states, idx, tx) {
                    Ok(update) => {
                        updates.push(update);
                    }
                    Err(e) => return Err(DaoError::StateTransitionFailed(e.to_string())),
                }
            } else if func_call.func_id == *dao::exec::FUNC_ID {
                debug!("dao_contract::exec::state_transition()");
                match dao::exec::validate::state_transition(&self.states, idx, tx) {
                    Ok(update) => {
                        updates.push(update);
                    }
                    Err(e) => return Err(DaoError::StateTransitionFailed(e.to_string())),
                }
            }
        }

        // Atomically apply all changes
        for update in updates {
            update.apply(&mut self.states);
        }

        tx.zk_verify(&self.zk_bins)?;
        tx.verify_sigs();

        Ok(())
    }

    fn update_wallets(&mut self) -> DaoResult<()> {
        let state = self
            .states
            .lookup_mut::<money::State>(*money::CONTRACT_ID)
            .ok_or(DaoError::StateNotFound)?;

        let dao_coins = state.wallet_cache.get_received(&self.dao_wallet.keypair.secret);
        for coin in dao_coins {
            let note = coin.note.clone();
            let (pub_x, pub_y) = self.dao_wallet.keypair.public.xy();

            let coin_hash = poseidon_hash::<8>([
                pub_x,
                pub_y,
                DrkValue::from(note.value),
                note.token_id.inner(),
                note.serial,
                note.spend_hook,
                note.user_data,
                note.coin_blind,
            ]);

            assert_eq!(coin_hash, coin.coin.0);
            assert_eq!(note.spend_hook, *dao::exec::FUNC_ID);
            assert_eq!(note.user_data, self.dao_wallet.bullas[0].0);

            self.dao_wallet.own_coins.push((coin, false));
            debug!("DAO received a coin worth {} xDRK", note.value);
        }

        for wallet in self.money_wallets.values_mut() {
            let coins = state.wallet_cache.get_received(&wallet.keypair.secret);
            for coin in coins {
                let note = coin.note.clone();
                let (pub_x, pub_y) = wallet.keypair.public.xy();

                let coin_hash = poseidon_hash::<8>([
                    pub_x,
                    pub_y,
                    DrkValue::from(note.value),
                    note.token_id.inner(),
                    note.serial,
                    note.spend_hook,
                    note.user_data,
                    note.coin_blind,
                ]);

                assert_eq!(coin_hash, coin.coin.0);
                wallet.own_coins.push((coin, false));
            }
        }

        Ok(())
    }

    fn propose(
        &mut self,
        recipient: PublicKey,
        token_id: TokenId,
        amount: u64,
        sender: PublicKey,
    ) -> DaoResult<pallas::Base> {
        let params = self.dao_wallet.params[0].clone();

        let dao_leaf_position = self.dao_wallet.leaf_position;

        // To be able to make a proposal, we must prove we have ownership
        // of governance tokens, and that the quantity of governance
        // tokens is within the accepted proposer limit.
        let sender_wallet = self.money_wallets.get_mut(&sender.to_bytes());
        if sender_wallet.is_none() {
            return Err(DaoError::NoWalletFound)
        }
        let sender_wallet = sender_wallet.unwrap();

        let tx = sender_wallet.propose_tx(
            params,
            recipient,
            token_id,
            amount,
            dao_leaf_position,
            &self.zk_bins,
            &mut self.states,
        )?;

        self.validate(&tx)?;
        self.update_wallets()?;

        let proposal_bulla = self.dao_wallet.store_proposal(&tx)?;

        Ok(proposal_bulla)
    }

    fn cast_vote(&mut self, pubkey: PublicKey, vote: bool) -> DaoResult<()> {
        let dao_key = self.dao_wallet.keypair;
        if self.dao_wallet.proposals.is_empty() {
            return Err(DaoError::NoProposals)
        }
        let proposal = self.dao_wallet.proposals[0].clone();

        if self.dao_wallet.params.is_empty() {
            return Err(DaoError::DaoNotConfigured)
        }
        let dao_params = self.dao_wallet.params[0].clone();
        let dao_keypair = self.dao_wallet.keypair;

        let voter_wallet = self.money_wallets.get_mut(&pubkey.to_bytes());
        if voter_wallet.is_none() {
            return Err(DaoError::NoWalletFound)
        }
        let voter_wallet = voter_wallet.unwrap();

        let tx = voter_wallet.vote_tx(
            vote,
            dao_key,
            proposal,
            dao_params,
            dao_keypair,
            &self.zk_bins,
            &mut self.states,
        )?;

        self.validate(&tx)?;
        self.update_wallets()?;

        self.dao_wallet.store_vote(&tx)?;

        Ok(())
    }

    fn exec_proposal(&mut self, bulla: pallas::Base) -> DaoResult<()> {
        if self.dao_wallet.proposals.is_empty() {
            return Err(DaoError::NoProposals)
        }
        let proposal = self.dao_wallet.proposals[0].clone();

        if self.dao_wallet.params.is_empty() {
            return Err(DaoError::DaoNotConfigured)
        }
        let dao_params = self.dao_wallet.params[0].clone();

        let tx = self.dao_wallet.exec_tx(
            proposal,
            bulla,
            dao_params,
            &self.zk_bins,
            &mut self.states,
        )?;

        self.validate(&tx)?;
        self.update_wallets()?;

        Ok(())
    }
}

struct DaoWallet {
    keypair: Keypair,
    signature_secret: SecretKey,
    bulla_blind: pallas::Base,
    leaf_position: Position,
    proposal_bullas: Vec<pallas::Base>,
    bullas: Vec<DaoBulla>,
    params: Vec<DaoParams>,
    own_coins: Vec<(OwnCoin, bool)>,
    proposals: Vec<Proposal>,
    vote_notes: Vec<dao::vote::wallet::Note>,
}
impl DaoWallet {
    fn new() -> Self {
        let keypair = Keypair::random(&mut OsRng);
        let signature_secret = SecretKey::random(&mut OsRng);
        let bulla_blind = pallas::Base::random(&mut OsRng);
        let leaf_position = Position::zero();
        let proposal_bullas = Vec::new();
        let bullas = Vec::new();
        let params = Vec::new();
        let own_coins: Vec<(OwnCoin, bool)> = Vec::new();
        let proposals: Vec<Proposal> = Vec::new();
        let vote_notes = Vec::new();

        Self {
            keypair,
            signature_secret,
            bulla_blind,
            leaf_position,
            proposal_bullas,
            bullas,
            params,
            own_coins,
            proposals,
            vote_notes,
        }
    }

    fn get_public_key(&self) -> PublicKey {
        self.keypair.public
    }

    fn track(&self, states: &mut StateRegistry) -> DaoResult<()> {
        let state = states
            .lookup_mut::<money::State>(*money::CONTRACT_ID)
            .ok_or(DaoError::StateNotFound)?;

        state.wallet_cache.track(self.keypair.secret);
        Ok(())
    }

    // Mint the DAO bulla.
    fn mint_tx(
        &mut self,
        dao_proposer_limit: u64,
        dao_quorum: u64,
        dao_approval_ratio_quot: u64,
        dao_approval_ratio_base: u64,
        token_id: TokenId,
        zk_bins: &ZkContractTable,
    ) -> Transaction {
        debug!(target: "dao-demo::dao::mint_tx()", "START");
        let builder = dao::mint::wallet::Builder {
            dao_proposer_limit,
            dao_quorum,
            dao_approval_ratio_quot,
            dao_approval_ratio_base,
            gov_token_id: token_id,
            dao_pubkey: self.keypair.public,
            dao_bulla_blind: self.bulla_blind,
            _signature_secret: self.signature_secret,
        };
        let func_call = builder.build(zk_bins);
        let func_calls = vec![func_call];

        let mut signatures = vec![];
        for func_call in &func_calls {
            let sig = sign(vec![self.signature_secret], func_call);
            signatures.push(sig);
        }

        Transaction { func_calls, signatures }
    }

    fn update_witness(&mut self, states: &mut StateRegistry) -> DaoResult<()> {
        let state =
            states.lookup_mut::<dao::State>(*dao::CONTRACT_ID).ok_or(DaoError::StateNotFound)?;

        let path =
            state.dao_tree.witness().ok_or_else(|| Error::Custom("Tree is empty".to_owned()))?;
        self.leaf_position = path;
        Ok(())
    }

    fn balances(&self) -> Result<HashMap<String, u64>> {
        let mut ret: HashMap<String, u64> = HashMap::new();
        for (coin, is_spent) in &self.own_coins {
            if *is_spent {}
            if coin.note.token_id == *DRK_ID || coin.note.token_id == *GOV_ID {
                let token_id = format!("{}", coin.note.token_id);
                ret.insert(token_id, coin.note.value);
            }
        }
        Ok(ret)
    }

    fn store_proposal(&mut self, tx: &Transaction) -> Result<pallas::Base> {
        let (proposal, proposal_bulla) = {
            let func_call = &tx.func_calls[0];
            let call_data = func_call.call_data.as_any();
            let call_data = call_data.downcast_ref::<dao::propose::validate::CallData>().unwrap();

            let header = &call_data.header;
            let note: dao::propose::wallet::Note = header.enc_note.decrypt(&self.keypair.secret)?;

            // Return the proposal info
            (note.proposal, call_data.header.proposal_bulla)
        };
        debug!(target: "demo", "Proposal now active!");
        debug!(target: "demo", "  destination: {:?}", proposal.dest);
        debug!(target: "demo", "  amount: {}", proposal.amount);
        debug!(target: "demo", "  token_id: {:?}", proposal.token_id);
        debug!(target: "demo", "Proposal bulla: {:?}", proposal_bulla);

        self.proposals.push(proposal);
        self.proposal_bullas.push(proposal_bulla);

        Ok(proposal_bulla)
    }

    // We decrypt the votes in a transaction and add it to the wallet.
    fn store_vote(&mut self, tx: &Transaction) -> Result<()> {
        let vote_note = {
            let func_call = &tx.func_calls[0];
            let call_data = func_call.call_data.as_any();
            let call_data = call_data.downcast_ref::<dao::vote::validate::CallData>().unwrap();

            let header = &call_data.header;
            let note: dao::vote::wallet::Note = header.enc_note.decrypt(&self.keypair.secret)?;
            note
        };

        self.vote_notes.push(vote_note);

        Ok(())
    }

    fn get_proposals(&self) -> &Vec<Proposal> {
        &self.proposals
    }

    fn get_votes(&self) -> &Vec<dao::vote::wallet::Note> {
        &self.vote_notes
    }

    fn get_treasury_path(
        &self,
        own_coin: &OwnCoin,
        states: &StateRegistry,
    ) -> DaoResult<(Position, Vec<MerkleNode>)> {
        let (money_leaf_position, money_merkle_path) = {
            let state = states
                .lookup::<money::State>(*money::CONTRACT_ID)
                .ok_or(DaoError::StateNotFound)?;

            let tree = &state.tree;
            let leaf_position = own_coin.leaf_position;
            let root = tree.root(0).ok_or_else(|| {
                Error::Custom(
                    "Not enough checkpoints available to reach the requested checkpoint depth."
                        .to_owned(),
                )
            })?;
            let merkle_path = tree
                .authentication_path(leaf_position, &root)
                .ok_or_else(|| Error::Custom("No available authentication path to that position or if the root does not correspond to a checkpointed root of the tree".to_owned()))?;
            (leaf_position, merkle_path)
        };

        Ok((money_leaf_position, money_merkle_path))
    }

    fn exec_tx(
        &self,
        proposal: Proposal,
        _proposal_bulla: pallas::Base,
        dao_params: DaoParams,
        zk_bins: &ZkContractTable,
        states: &mut StateRegistry,
    ) -> DaoResult<Transaction> {
        let dao_bulla = self.bullas[0].clone();

        let mut inputs = Vec::new();
        let mut total_input_value = 0;

        let tx_signature_secret = SecretKey::random(&mut OsRng);
        let exec_signature_secret = SecretKey::random(&mut OsRng);

        let user_serial = pallas::Base::random(&mut OsRng);
        let user_coin_blind = pallas::Base::random(&mut OsRng);
        let user_data_blind = pallas::Base::random(&mut OsRng);
        let input_value_blind = pallas::Scalar::random(&mut OsRng);
        let dao_serial = pallas::Base::random(&mut OsRng);
        let dao_coin_blind = pallas::Base::random(&mut OsRng);
        // disabled
        let user_spend_hook = pallas::Base::from(0);
        let user_data = pallas::Base::from(0);

        for (coin, is_spent) in &self.own_coins {
            let is_spent = *is_spent;
            if is_spent {
                continue
            }
            let (treasury_leaf_position, treasury_merkle_path) =
                self.get_treasury_path(coin, states)?;

            let input_value = coin.note.value;

            let input = {
                money::transfer::wallet::BuilderInputInfo {
                    leaf_position: treasury_leaf_position,
                    merkle_path: treasury_merkle_path,
                    secret: self.keypair.secret,
                    note: coin.note.clone(),
                    user_data_blind,
                    value_blind: input_value_blind,
                    signature_secret: tx_signature_secret,
                }
            };
            total_input_value += input_value;
            inputs.push(input);
        }

        let builder = {
            money::transfer::wallet::Builder {
                clear_inputs: vec![],
                inputs,
                outputs: vec![
                    // Sending money
                    money::transfer::wallet::BuilderOutputInfo {
                        value: proposal.amount,
                        token_id: proposal.token_id,
                        public: proposal.dest,
                        serial: proposal.serial,
                        coin_blind: proposal.blind,
                        spend_hook: user_spend_hook,
                        user_data,
                    },
                    // Change back to DAO
                    money::transfer::wallet::BuilderOutputInfo {
                        value: total_input_value - proposal.amount,
                        token_id: *DRK_ID,
                        public: self.keypair.public,
                        serial: dao_serial,
                        coin_blind: dao_coin_blind,
                        spend_hook: *dao::exec::FUNC_ID,
                        user_data: dao_bulla.0,
                    },
                ],
            }
        };

        let transfer_func_call = builder.build(zk_bins)?;

        let mut yes_votes_value = 0;
        let mut yes_votes_blind = pallas::Scalar::from(0);
        let mut yes_votes_commit = pallas::Point::identity();

        let mut all_votes_value = 0;
        let mut all_votes_blind = pallas::Scalar::from(0);
        let mut all_votes_commit = pallas::Point::identity();

        for (i, note) in self.vote_notes.iter().enumerate() {
            let vote_commit = pedersen_commitment_u64(note.vote_value, note.vote_value_blind);

            all_votes_commit += vote_commit;
            all_votes_blind += note.vote_value_blind;

            let yes_vote_commit = pedersen_commitment_u64(
                note.vote.vote_option as u64 * note.vote_value,
                note.vote.vote_option_blind,
            );

            yes_votes_commit += yes_vote_commit;
            yes_votes_blind += note.vote.vote_option_blind;

            let vote_option = note.vote.vote_option;

            if vote_option {
                yes_votes_value += note.vote_value;
            }
            all_votes_value += note.vote_value;
            let vote_result: String =
                if vote_option { "yes".to_string() } else { "no".to_string() };

            debug!("Voter {} voted {}", i, vote_result);
        }

        debug!("Outcome = {} / {}", yes_votes_value, all_votes_value);

        assert!(all_votes_commit == pedersen_commitment_u64(all_votes_value, all_votes_blind));
        assert!(yes_votes_commit == pedersen_commitment_u64(yes_votes_value, yes_votes_blind));

        let builder = {
            dao::exec::wallet::Builder {
                proposal,
                dao: dao_params,
                yes_votes_value,
                all_votes_value,
                yes_votes_blind,
                all_votes_blind,
                user_serial,
                user_coin_blind,
                dao_serial,
                dao_coin_blind,
                input_value: total_input_value,
                input_value_blind,
                hook_dao_exec: *dao::exec::FUNC_ID,
                signature_secret: exec_signature_secret,
            }
        };

        let exec_func_call = builder.build(zk_bins);
        let func_calls = vec![transfer_func_call, exec_func_call];

        let mut signatures = vec![];
        for func_call in &func_calls {
            let sig = sign(vec![self.signature_secret], func_call);
            signatures.push(sig);
        }

        Ok(Transaction { func_calls, signatures })
    }
}

// Stores governance tokens and related secret values.
struct MoneyWallet {
    keypair: Keypair,
    signature_secret: SecretKey,
    own_coins: Vec<(OwnCoin, bool)>,
}

impl MoneyWallet {
    // fn signature_public(&self) -> PublicKey {
    //     PublicKey::from_secret(self.signature_secret)
    // }

    // fn get_public_key(&self) -> PublicKey {
    //     self.keypair.public
    // }

    fn track(&self, states: &mut StateRegistry) -> DaoResult<()> {
        let state = states
            .lookup_mut::<money::State>(*money::CONTRACT_ID)
            .ok_or(DaoError::StateNotFound)?;

        state.wallet_cache.track(self.keypair.secret);
        Ok(())
    }

    fn balances(&self) -> Result<HashMap<String, u64>> {
        let mut ret: HashMap<String, u64> = HashMap::new();
        for (coin, is_spent) in &self.own_coins {
            if *is_spent {}
            if coin.note.token_id == *DRK_ID || coin.note.token_id == *GOV_ID {
                let token_id = format!("{}", coin.note.token_id);
                ret.insert(token_id, coin.note.value);
            }
        }
        Ok(ret)
    }

    fn propose_tx(
        &mut self,
        params: DaoParams,
        recipient: PublicKey,
        token_id: TokenId,
        amount: u64,
        dao_leaf_position: Position,
        zk_bins: &ZkContractTable,
        states: &mut StateRegistry,
    ) -> DaoResult<Transaction> {
        let mut inputs = Vec::new();

        for (coin, is_spent) in &self.own_coins {
            let is_spent = *is_spent;
            if is_spent {
                continue
            }
            let (money_leaf_position, money_merkle_path) = self.get_path(states, coin).unwrap();

            let input = {
                dao::propose::wallet::BuilderInput {
                    secret: self.keypair.secret,
                    note: coin.note.clone(),
                    leaf_position: money_leaf_position,
                    merkle_path: money_merkle_path,
                    signature_secret: self.signature_secret,
                }
            };
            inputs.push(input);
        }

        let (dao_merkle_path, dao_merkle_root) = {
            let state =
                states.lookup::<dao::State>(*dao::CONTRACT_ID).ok_or(DaoError::StateNotFound)?;
            let tree = &state.dao_tree;
            let root = tree.root(0).ok_or_else(|| {
                Error::Custom(
                    "Not enough checkpoints available to reach the requested checkpoint depth."
                        .to_owned(),
                )
            })?;
            let merkle_path = tree.authentication_path(dao_leaf_position, &root)
            .ok_or_else(|| Error::Custom(
                "No available authentication path to that position or if the root does not correspond to a checkpointed root of the tree"
                .to_owned()
            ))?;
            (merkle_path, root)
        };

        let proposal = {
            dao::propose::wallet::Proposal {
                dest: recipient,
                amount,
                serial: pallas::Base::random(&mut OsRng),
                token_id,
                blind: pallas::Base::random(&mut OsRng),
            }
        };

        let builder = dao::propose::wallet::Builder {
            inputs,
            proposal,
            dao: params,
            dao_leaf_position,
            dao_merkle_path,
            dao_merkle_root,
        };

        let func_call = builder.build(zk_bins);
        let func_calls = vec![func_call];

        let mut signatures = vec![];
        for func_call in &func_calls {
            let sig = sign(vec![self.signature_secret], func_call);
            signatures.push(sig);
        }

        Ok(Transaction { func_calls, signatures })
    }

    fn get_path(
        &self,
        states: &StateRegistry,
        own_coin: &OwnCoin,
    ) -> DaoResult<(Position, Vec<MerkleNode>)> {
        let (money_leaf_position, money_merkle_path) = {
            let state = states
                .lookup::<money::State>(*money::CONTRACT_ID)
                .ok_or(DaoError::StateNotFound)?;

            let tree = &state.tree;
            let leaf_position = own_coin.leaf_position;
            let root = tree.root(0).ok_or_else(|| {
                Error::Custom(
                    "Not enough checkpoints available to reach the requested checkpoint depth."
                        .to_owned(),
                )
            })?;
            let merkle_path = tree.authentication_path(leaf_position, &root).ok_or_else(|| Error::Custom(
                "No available authentication path to that position or the root does not correspond to a checkpointed root of the tree"
                .to_owned()
            ))?;
            (leaf_position, merkle_path)
        };

        Ok((money_leaf_position, money_merkle_path))
    }

    fn vote_tx(
        &mut self,
        vote_option: bool,
        _dao_key: Keypair,
        proposal: Proposal,
        dao_params: DaoParams,
        dao_keypair: Keypair,
        zk_bins: &ZkContractTable,
        states: &mut StateRegistry,
    ) -> Result<Transaction> {
        let mut inputs = Vec::new();

        // We must prove we have sufficient governance tokens in order to vote.
        for (coin, _is_spent) in &self.own_coins {
            let (money_leaf_position, money_merkle_path) = self.get_path(states, coin).unwrap();

            let input = {
                dao::vote::wallet::BuilderInput {
                    secret: self.keypair.secret,
                    note: coin.note.clone(),
                    leaf_position: money_leaf_position,
                    merkle_path: money_merkle_path,
                    signature_secret: self.signature_secret,
                }
            };
            inputs.push(input);
        }

        let builder = {
            dao::vote::wallet::Builder {
                inputs,
                vote: dao::vote::wallet::Vote {
                    vote_option,
                    vote_option_blind: pallas::Scalar::random(&mut OsRng),
                },
                // For this demo votes are encrypted for the DAO.
                vote_keypair: dao_keypair,
                proposal,
                dao: dao_params,
            }
        };

        let func_call = builder.build(zk_bins);
        let func_calls = vec![func_call];

        let mut signatures = vec![];
        for func_call in &func_calls {
            let sig = sign(vec![self.signature_secret], func_call);
            signatures.push(sig);
        }

        Ok(Transaction { func_calls, signatures })
    }
}

async fn start_rpc(client: Client) -> Result<()> {
    let rpc_addr = Url::parse("tcp://127.0.0.1:7777")?;

    let rpc_client = JsonRpcInterface::new(client);

    let rpc_interface = Arc::new(rpc_client);

    listen_and_serve(rpc_addr, rpc_interface).await?;
    Ok(())
}

// Mint authority that mints the DAO treasury and airdrops governance tokens.
#[derive(Clone)]
struct CashierWallet {
    // keypair: Keypair,
    signature_secret: SecretKey,
}

impl CashierWallet {
    fn new() -> Self {
        // let keypair = Keypair::random(&mut OsRng);
        let signature_secret = SecretKey::random(&mut OsRng);
        // Self { keypair, signature_secret }
        Self { signature_secret }
    }

    fn signature_public(&self) -> PublicKey {
        PublicKey::from_secret(self.signature_secret)
    }

    fn mint(
        &mut self,
        token_id: TokenId,
        token_supply: u64,
        dao_bulla: pallas::Base,
        recipient: PublicKey,
        zk_bins: &ZkContractTable,
    ) -> Result<Transaction> {
        let spend_hook = *dao::exec::FUNC_ID;
        let user_data = dao_bulla;
        let value = token_supply;

        let tx = self.transfer_tx(value, token_id, spend_hook, user_data, recipient, zk_bins)?;

        Ok(tx)
    }

    fn transfer_tx(
        &self,
        value: u64,
        token_id: TokenId,
        spend_hook: pallas::Base,
        user_data: pallas::Base,
        recipient: PublicKey,
        zk_bins: &ZkContractTable,
    ) -> Result<Transaction> {
        let builder = {
            money::transfer::wallet::Builder {
                clear_inputs: vec![money::transfer::wallet::BuilderClearInputInfo {
                    value,
                    token_id,
                    signature_secret: self.signature_secret,
                }],
                inputs: vec![],
                outputs: vec![money::transfer::wallet::BuilderOutputInfo {
                    value,
                    token_id,
                    public: recipient,
                    serial: pallas::Base::random(&mut OsRng),
                    coin_blind: pallas::Base::random(&mut OsRng),
                    spend_hook,
                    user_data,
                }],
            }
        };
        let func_call = builder.build(zk_bins)?;
        let func_calls = vec![func_call];

        let mut signatures = vec![];
        for func_call in &func_calls {
            let sig = sign(vec![self.signature_secret], func_call);
            signatures.push(sig);
        }

        Ok(Transaction { func_calls, signatures })
    }

    fn airdrop(
        &mut self,
        value: u64,
        token_id: TokenId,
        recipient: PublicKey,
        zk_bins: &ZkContractTable,
    ) -> Result<Transaction> {
        // Spend hook and user data disabled
        let spend_hook = DrkSpendHook::from(0);
        let user_data = DrkUserData::from(0);

        let tx = self.transfer_tx(value, token_id, spend_hook, user_data, recipient, zk_bins)?;

        Ok(tx)
    }
}

#[async_std::main]
async fn main() -> Result<()> {
    TermLogger::init(
        LevelFilter::Debug,
        simplelog::Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )?;

    let mut client = Client::new();
    client.init()?;

    start_rpc(client).await?;

    Ok(())
}
