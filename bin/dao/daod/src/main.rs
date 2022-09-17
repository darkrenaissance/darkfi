use std::{any::TypeId, sync::Arc, time::Instant};

use incrementalmerkletree::{Position, Tree};
use log::debug;
use pasta_curves::{
    arithmetic::CurveAffine,
    group::{ff::Field, Curve, Group},
    pallas, Fp, Fq,
};
use rand::rngs::OsRng;
use simplelog::{ColorChoice, LevelFilter, TermLogger, TerminalMode};
use url::Url;

use darkfi::{
    crypto::{
        keypair::{Keypair, PublicKey, SecretKey},
        merkle_node::MerkleNode,
        proof::{ProvingKey, VerifyingKey},
        types::{DrkSpendHook, DrkUserData, DrkValue},
        util::{pedersen_commitment_u64, poseidon_hash},
    },
    rpc::server::listen_and_serve,
    zk::circuit::{BurnContract, MintContract},
    zkas::ZkBinary,
    Result,
};

mod contract;
mod note;
mod rpc;
mod util;

use crate::{
    contract::{
        dao_contract::{self, mint::wallet::DaoParams, propose::wallet::Proposal, DaoBulla},
        money_contract::{self, state::OwnCoin, transfer::Note},
    },
    rpc::JsonRpcInterface,
    util::{sign, FuncCall, StateRegistry, Transaction, ZkContractTable, GDRK_ID, XDRK_ID},
};

/////////////////////////////////////////////////////////////////////////////////////////
// TODO: restructure to this architecture.
// Note: to make a Proposal, you need the dao_leaf_position
// to make a Vote, you need the dao decryption key
// Everyone has a unique money_wallet and a copy of the dao_wallet in their Client.
//
// pub struct Cashier {
//      cashier_wallet: CashierWallet,
//      zk_bins, ...
//      states ...
// }
//
// impl Cashier {
//      init() ...
//      mint_treasury()...
//      airdrop() ...
// }
//
// pub struct Dao {
//      dao_params: DaoParams,
//      dao_wallet: DaoWallet,
// }
//
// pub struct Client {
//      dao: Dao,
//      money_wallet: MoneyWallet,
// }
//
// fn start() {
//      cashier::init();
//
//      match input {
//          dao_create() => Dao::new()
//          wallet_create() => Client::new(money_wallet::new(), dao)
//      }
// }

pub struct Client {
    dao_wallet: DaoWallet,
    money_wallet: MoneyWallet,
    states: StateRegistry,
    zk_bins: ZkContractTable,
}

impl Client {
    // TODO: user passes DAO approval ratio: 1/2
    // we parse that into dao_approval_ratio_base and dao_approval_ratio_quot
    fn create_dao(
        &mut self,
        dao_proposer_limit: u64,
        dao_quorum: u64,
        dao_approval_ratio_quot: u64,
        dao_approval_ratio_base: u64,
        token_id: pallas::Base,
    ) -> pallas::Base {
        let tx = self.dao_wallet.build_mint_tx(
            dao_proposer_limit,
            dao_quorum,
            dao_approval_ratio_quot,
            dao_approval_ratio_base,
            token_id,
            &self.zk_bins,
        );

        self.validate(&tx);

        self.dao_wallet.balances(&mut self.states);

        let dao_bulla = {
            assert_eq!(tx.func_calls.len(), 1);
            let func_call = &tx.func_calls[0];
            let call_data = func_call.call_data.as_any();
            assert_eq!(
                (&*call_data).type_id(),
                TypeId::of::<dao_contract::mint::validate::CallData>()
            );
            let call_data =
                call_data.downcast_ref::<dao_contract::mint::validate::CallData>().unwrap();
            call_data.dao_bulla.clone()
        };

        debug!(target: "demo", "Create DAO bulla: {:?}", dao_bulla.0);

        dao_bulla.0
    }

    fn init(&mut self) -> Result<()> {
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

        // State for money contracts
        // TODO: we need the cashier value elsewhere.
        let cashier_signature_secret = SecretKey::random(&mut OsRng);
        let cashier_signature_public = PublicKey::from_secret(cashier_signature_secret);
        let faucet_signature_secret = SecretKey::random(&mut OsRng);
        let faucet_signature_public = PublicKey::from_secret(faucet_signature_secret);

        ///////////////////////////////////////////////////
        let money_state =
            money_contract::state::State::new(cashier_signature_public, faucet_signature_public);
        self.states.register(*money_contract::CONTRACT_ID, money_state);
        /////////////////////////////////////////////////////
        let dao_state = dao_contract::State::new();
        self.states.register(*dao_contract::CONTRACT_ID, dao_state);
        /////////////////////////////////////////////////////

        Ok(())
    }

    // TODO: user passes "gDRK", we match with gDRK tokenID
    fn mint_treasury(
        &mut self,
        token_id: pallas::Base,
        token_supply: u64,
        dao_bulla: pallas::Base,
        recipient: PublicKey,
    ) -> Result<()> {
        let spend_hook = *dao_contract::exec::FUNC_ID;

        let user_data = dao_bulla;
        let value = token_supply;
        let tx = self.money_wallet.build_transfer_tx(
            value,
            token_id,
            spend_hook,
            user_data,
            recipient,
            &self.zk_bins,
        )?;

        self.validate(&tx);

        let own_coin = self.dao_wallet.balances(&mut self.states)?;

        // TODO: return own_coin.note.value to CLI

        Ok(())
    }

    fn airdrop(&mut self, value: u64, token_id: pallas::Base, recipient: PublicKey) -> Result<()> {
        // Spend hook and user data disabled
        let spend_hook = DrkSpendHook::from(0);
        let user_data = DrkUserData::from(0);

        let tx = self.money_wallet.build_transfer_tx(
            value,
            token_id,
            spend_hook,
            user_data,
            recipient,
            &self.zk_bins,
        )?;

        self.validate(&tx);

        let own_coin = self.money_wallet.balances(&mut self.states)?;

        // TODO: return own_coin.note.value to CLI

        Ok(())
    }

    fn validate(&mut self, tx: &Transaction) -> Result<()> {
        let mut updates = vec![];

        let states = &self.states;
        // Validate all function calls in the tx
        for (idx, func_call) in tx.func_calls.iter().enumerate() {
            // So then the verifier will lookup the corresponding state_transition and apply
            // functions based off the func_id

            if func_call.func_id == *money_contract::transfer::FUNC_ID {
                debug!("money_contract::transfer::state_transition()");
                let update = money_contract::transfer::validate::state_transition(states, idx, &tx)
                    .expect("money_contract::transfer::validate::state_transition() failed!");
                updates.push(update);
            } else if func_call.func_id == *dao_contract::mint::FUNC_ID {
                debug!("dao_contract::mint::state_transition()");
                let update = dao_contract::mint::validate::state_transition(states, idx, &tx)
                    .expect("dao_contract::mint::validate::state_transition() failed!");
                updates.push(update);
            } else if func_call.func_id == *dao_contract::propose::FUNC_ID {
                debug!(target: "demo", "dao_contract::propose::state_transition()");
                let update = dao_contract::propose::validate::state_transition(states, idx, &tx)
                    .expect("dao_contract::propose::validate::state_transition() failed!");
                updates.push(update);
            } else if func_call.func_id == *dao_contract::vote::FUNC_ID {
                debug!(target: "demo", "dao_contract::vote::state_transition()");
                let update = dao_contract::vote::validate::state_transition(states, idx, &tx)
                    .expect("dao_contract::vote::validate::state_transition() failed!");
                updates.push(update);
            } else if func_call.func_id == *dao_contract::exec::FUNC_ID {
                debug!("dao_contract::exec::state_transition()");
                let update = dao_contract::exec::validate::state_transition(states, idx, &tx)
                    .expect("dao_contract::exec::validate::state_transition() failed!");
                updates.push(update);
            }
        }

        // Atomically apply all changes
        for update in updates {
            update.apply(&mut self.states);
        }

        tx.zk_verify(&self.zk_bins);
        tx.verify_sigs();

        Ok(())
    }

    fn propose(
        &mut self,
        params: DaoParams,
        recipient: PublicKey,
        token_id: pallas::Base,
        amount: u64,
    ) -> Result<()> {
        let dao_leaf_position = self.dao_wallet.witness(&mut self.states)?;

        let tx = self.money_wallet.build_propose_tx(
            &mut self.states,
            &self.zk_bins,
            params,
            recipient,
            token_id,
            amount,
            dao_leaf_position,
        )?;

        self.validate(&tx)?;

        self.dao_wallet.read_proposal(&tx)?;

        Ok(())
    }

    // TODO: User must have the values Proposal and DaoParams in order to cast a vote.
    // These should be encoded to base58 and printed to command-line when a DAO is made (DaoParams)
    // and a Proposal is made (Proposal). Then the user loads a base58 string into the vote request.
    fn vote(&mut self, vote_option: bool, proposal: Proposal, dao_params: DaoParams) -> Result<()> {
        let dao_keypair = self.dao_wallet.get_vote_decryption_key();

        let tx = self.money_wallet.build_vote_tx(
            vote_option,
            &mut self.states,
            &self.zk_bins,
            dao_keypair,
            proposal,
            dao_params,
        )?;

        self.validate(&tx)?;

        self.dao_wallet.read_vote(&tx)?;

        Ok(())
    }

    // TODO: user must pass in a base58 encoded string of the Proposal, proposal_bulla and
    // DaoParams
    fn exec(
        &mut self,
        proposal: Proposal,
        proposal_bulla: pallas::Base,
        dao_params: DaoParams,
    ) -> Result<()> {
        self.dao_wallet.build_exec_tx(
            &mut self.states,
            &self.zk_bins,
            proposal,
            proposal_bulla,
            dao_params,
        )?;
        Ok(())
    }
}

// DAO private values. This class is purely concerned with the DAO treasury.
// TODO: we must call track() for keypairs before we can query the balance.
pub struct DaoWallet {
    keypair: Keypair,
    vote_keypair: Keypair,
    signature: SecretKey,
    params: DaoParams,
    vote_notes: Vec<dao_contract::vote::wallet::Note>,
}

impl DaoWallet {
    fn read_proposal(&self, tx: &Transaction) -> Result<()> {
        let (proposal, proposal_bulla) = {
            let func_call = &tx.func_calls[0];
            let call_data = func_call.call_data.as_any();
            let call_data =
                call_data.downcast_ref::<dao_contract::propose::validate::CallData>().unwrap();

            let header = &call_data.header;
            let note: dao_contract::propose::wallet::Note =
                header.enc_note.decrypt(&self.keypair.secret).unwrap();

            // Return the proposal info
            (note.proposal, call_data.header.proposal_bulla)
        };
        // TODO: this should print from the CLI rather than use debug statements.
        debug!(target: "demo", "Proposal now active!");
        debug!(target: "demo", "  destination: {:?}", proposal.dest);
        debug!(target: "demo", "  amount: {}", proposal.amount);
        debug!(target: "demo", "  token_id: {:?}", proposal.token_id);
        debug!(target: "demo", "Proposal bulla: {:?}", proposal_bulla);

        Ok(())

        // TODO: encode Proposal as base58 and return to cli
    }

    // We decrypt the votes in a transaction and add it to the wallet.
    fn read_vote(&mut self, tx: &Transaction) -> Result<()> {
        let vote_note = {
            let func_call = &tx.func_calls[0];
            let call_data = func_call.call_data.as_any();
            let call_data =
                call_data.downcast_ref::<dao_contract::vote::validate::CallData>().unwrap();

            let header = &call_data.header;
            let note: dao_contract::vote::wallet::Note =
                header.enc_note.decrypt(&self.vote_keypair.secret).unwrap();
            note
        };

        self.vote_notes.push(vote_note);

        // TODO: this should print from the CLI rather than use debug statements.
        // TODO: maybe this its own method? get votes
        //debug!(target: "demo", "User voted!");
        //debug!(target: "demo", "  vote_option: {}", vote_note.vote.vote_option);
        //debug!(target: "demo", "  value: {}", vote_note.vote_value);

        Ok(())
    }

    // We need to encrypt votes to the DAO secret key for this demo, which requires users
    // to have access to a secret key operated by the DAO. We create a specific key for decrypting
    // votes which is different to the key that operates the treasury.
    fn get_vote_decryption_key(&self) -> Keypair {
        self.vote_keypair
    }

    fn track(&self, states: &mut StateRegistry) -> Result<()> {
        let state =
            states.lookup_mut::<money_contract::State>(*money_contract::CONTRACT_ID).unwrap();
        state.wallet_cache.track(self.keypair.secret);

        Ok(())
    }

    fn witness(&self, states: &mut StateRegistry) -> Result<Position> {
        let state = states.lookup_mut::<dao_contract::State>(*dao_contract::CONTRACT_ID).unwrap();
        let path = state.dao_tree.witness();
        // TODO: error handling
        //if path.is_some() {
        return Ok(path.unwrap())
        //}
    }

    fn balances(&self, states: &mut StateRegistry) -> Result<OwnCoin> {
        let state =
            states.lookup_mut::<money_contract::State>(*money_contract::CONTRACT_ID).unwrap();

        let mut recv_coins = state.wallet_cache.get_received(&self.keypair.secret);

        let dao_recv_coin = recv_coins.pop().unwrap();
        let treasury_note = dao_recv_coin.note.clone();

        debug!("DAO received a coin worth {} xDRK", treasury_note.value);

        Ok(dao_recv_coin)
    }

    // TODO: encode this to base58 and display in cli
    fn params(&self) -> Result<&DaoParams> {
        Ok(&self.params)
    }

    fn build_mint_tx(
        &self,
        dao_proposer_limit: u64,
        dao_quorum: u64,
        dao_approval_ratio_quot: u64,
        dao_approval_ratio_base: u64,
        token_id: pallas::Base,
        zk_bins: &ZkContractTable,
    ) -> Transaction {
        // TODO: store this?
        let dao_bulla_blind = pallas::Base::random(&mut OsRng);

        let builder = dao_contract::mint::wallet::Builder {
            dao_proposer_limit,
            dao_quorum,
            dao_approval_ratio_quot,
            dao_approval_ratio_base,
            gov_token_id: *GDRK_ID,
            dao_pubkey: self.keypair.public,
            dao_bulla_blind,
            _signature_secret: self.signature,
        };
        let func_call = builder.build(zk_bins);
        let func_calls = vec![func_call];

        // TODO: this should be a cashier key?
        let signatures = sign(vec![self.signature], &func_calls);
        let tx = Transaction { func_calls, signatures };
        tx
    }

    // We use this to prove ownership of treasury tokens.
    // Right now this method is duplicated on both wallets but doesn't need to be.
    // TODO: clean up the architecture.
    fn coin_path(
        &self,
        states: &StateRegistry,
        own_coin: &OwnCoin,
    ) -> Result<(Position, Vec<MerkleNode>)> {
        let (money_leaf_position, money_merkle_path) = {
            let state =
                states.lookup::<money_contract::State>(*money_contract::CONTRACT_ID).unwrap();
            let tree = &state.tree;
            let leaf_position = own_coin.leaf_position.clone();
            let root = tree.root(0).unwrap();
            let merkle_path = tree.authentication_path(leaf_position, &root).unwrap();
            (leaf_position, merkle_path)
        };

        Ok((money_leaf_position, money_merkle_path))
    }

    fn build_exec_tx(
        &self,
        states: &mut StateRegistry,
        zk_bins: &ZkContractTable,
        proposal: Proposal,
        proposal_bulla: pallas::Base,
        dao_params: DaoParams,
    ) -> Result<Transaction> {
        let own_coin = self.balances(states)?;

        let (treasury_leaf_position, treasury_merkle_path) = self.coin_path(states, &own_coin)?;

        let input_value = own_coin.note.value;

        // TODO: not sure what this is doing
        let user_serial = pallas::Base::random(&mut OsRng);
        let user_coin_blind = pallas::Base::random(&mut OsRng);
        let user_data_blind = pallas::Base::random(&mut OsRng);
        let input_value_blind = pallas::Scalar::random(&mut OsRng);
        let dao_serial = pallas::Base::random(&mut OsRng);
        let dao_coin_blind = pallas::Base::random(&mut OsRng);

        let input = {
            money_contract::transfer::wallet::BuilderInputInfo {
                leaf_position: treasury_leaf_position,
                merkle_path: treasury_merkle_path,
                secret: self.keypair.secret,
                note: own_coin.note.clone(),
                user_data_blind,
                value_blind: input_value_blind,
                // TODO: in schema, we create random signatures here. why?
                signature_secret: self.signature,
            }
        };

        let builder = {
            money_contract::transfer::wallet::Builder {
                clear_inputs: vec![],
                inputs: vec![input],
                outputs: vec![
                    // Sending money
                    money_contract::transfer::wallet::BuilderOutputInfo {
                        value: proposal.amount,
                        token_id: proposal.token_id,
                        public: proposal.dest,
                        serial: proposal.serial,
                        coin_blind: proposal.blind,
                        spend_hook: pallas::Base::from(0),
                        user_data: pallas::Base::from(0),
                    },
                    // Change back to DAO
                    money_contract::transfer::wallet::BuilderOutputInfo {
                        value: own_coin.note.value - proposal.amount,
                        token_id: own_coin.note.token_id,
                        public: self.keypair.public,
                        // ?
                        serial: dao_serial,
                        coin_blind: dao_coin_blind,
                        spend_hook: *dao_contract::exec::FUNC_ID,
                        user_data: proposal_bulla,
                    },
                ],
            }
        };

        let transfer_func_call = builder.build(zk_bins)?;

        let mut yes_votes_value = 0;
        let mut yes_votes_blind = pallas::Scalar::from(0);

        let mut all_votes_value = 0;
        let mut all_votes_blind = pallas::Scalar::from(0);

        for note in &self.vote_notes {
            if note.vote.vote_option {
                // this is a yes vote
                yes_votes_value += note.vote_value;
                yes_votes_blind += note.vote_value_blind;
            }
            all_votes_value += note.vote_value;
            all_votes_blind += note.vote_value_blind;
        }

        let builder = {
            dao_contract::exec::wallet::Builder {
                proposal: proposal.clone(),
                dao: dao_params.clone(),
                yes_votes_value,
                all_votes_value,
                yes_votes_blind,
                all_votes_blind,
                user_serial,
                user_coin_blind,
                dao_serial,
                dao_coin_blind,
                input_value: proposal.amount,
                input_value_blind,
                hook_dao_exec: *dao_contract::exec::FUNC_ID,
                signature_secret: self.signature,
            }
        };

        let exec_func_call = builder.build(zk_bins);
        let func_calls = vec![transfer_func_call, exec_func_call];

        // TODO: we sign both transactions with the same sig, is this wrong?
        let signatures = sign(vec![self.signature, self.signature], &func_calls);
        Ok(Transaction { func_calls, signatures })
    }
}
// Money private values.
pub struct MoneyWallet {
    keypair: Keypair,
    signature: SecretKey,
}

impl MoneyWallet {
    fn track(&self, states: &mut StateRegistry) -> Result<()> {
        let state =
            states.lookup_mut::<money_contract::State>(*money_contract::CONTRACT_ID).unwrap();
        state.wallet_cache.track(self.keypair.secret);

        Ok(())
    }

    fn balances(&self, states: &mut StateRegistry) -> Result<OwnCoin> {
        let state =
            states.lookup_mut::<money_contract::State>(*money_contract::CONTRACT_ID).unwrap();

        let mut recv_coins = state.wallet_cache.get_received(&self.keypair.secret);

        let recv_coin = recv_coins.pop().unwrap();
        let note = recv_coin.note.clone();

        debug!("User received a coin worth {} gDRK", note.value);

        Ok(recv_coin)
    }

    // We use this to prove ownership of governance tokens.
    fn coin_path(
        &self,
        states: &StateRegistry,
        own_coin: &OwnCoin,
    ) -> Result<(Position, Vec<MerkleNode>)> {
        let (money_leaf_position, money_merkle_path) = {
            let state =
                states.lookup::<money_contract::State>(*money_contract::CONTRACT_ID).unwrap();
            let tree = &state.tree;
            let leaf_position = own_coin.leaf_position.clone();
            let root = tree.root(0).unwrap();
            let merkle_path = tree.authentication_path(leaf_position, &root).unwrap();
            (leaf_position, merkle_path)
        };

        Ok((money_leaf_position, money_merkle_path))
    }

    fn build_transfer_tx(
        &self,
        value: u64,
        token_id: pallas::Base,
        spend_hook: pallas::Base,
        user_data: pallas::Base,
        recipient: PublicKey,
        zk_bins: &ZkContractTable,
    ) -> Result<Transaction> {
        let builder = {
            money_contract::transfer::wallet::Builder {
                clear_inputs: vec![money_contract::transfer::wallet::BuilderClearInputInfo {
                    value,
                    token_id,
                    signature_secret: self.signature,
                }],
                inputs: vec![],
                outputs: vec![money_contract::transfer::wallet::BuilderOutputInfo {
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

        let signatures = sign(vec![self.signature], &func_calls);
        Ok(Transaction { func_calls, signatures })
    }

    fn build_propose_tx(
        &mut self,
        states: &mut StateRegistry,
        zk_bins: &ZkContractTable,
        params: DaoParams,
        recipient: PublicKey,
        token_id: pallas::Base,
        amount: u64,
        dao_leaf_position: Position,
    ) -> Result<Transaction> {
        let own_coin = self.balances(states)?;

        let (money_leaf_position, money_merkle_path) = self.coin_path(&states, &own_coin)?;

        let signature_secret = SecretKey::random(&mut OsRng);

        let input = {
            dao_contract::propose::wallet::BuilderInput {
                secret: self.keypair.secret,
                note: own_coin.note.clone(),
                leaf_position: money_leaf_position,
                merkle_path: money_merkle_path,
                signature_secret,
            }
        };

        let (dao_merkle_path, dao_merkle_root) = {
            let state = states.lookup::<dao_contract::State>(*dao_contract::CONTRACT_ID).unwrap();
            let tree = &state.dao_tree;
            let root = tree.root(0).unwrap();
            let merkle_path = tree.authentication_path(dao_leaf_position, &root).unwrap();
            (merkle_path, root)
        };

        let proposal = {
            dao_contract::propose::wallet::Proposal {
                dest: recipient,
                amount,
                serial: pallas::Base::random(&mut OsRng),
                token_id,
                blind: pallas::Base::random(&mut OsRng),
            }
        };

        let builder = dao_contract::propose::wallet::Builder {
            inputs: vec![input],
            proposal,
            dao: params.clone(),
            dao_leaf_position,
            dao_merkle_path,
            dao_merkle_root,
        };

        let func_call = builder.build(zk_bins);
        let func_calls = vec![func_call];

        let signatures = sign(vec![signature_secret], &func_calls);
        Ok(Transaction { func_calls, signatures })
    }

    fn build_vote_tx(
        &mut self,
        vote_option: bool,
        states: &mut StateRegistry,
        zk_bins: &ZkContractTable,
        dao_key: Keypair,
        proposal: Proposal,
        dao_params: DaoParams,
    ) -> Result<Transaction> {
        let own_coin = self.balances(states)?;

        let (money_leaf_position, money_merkle_path) = self.coin_path(states, &own_coin)?;

        let input = {
            dao_contract::vote::wallet::BuilderInput {
                secret: self.keypair.secret,
                note: own_coin.note.clone(),
                leaf_position: money_leaf_position,
                merkle_path: money_merkle_path,
                signature_secret: self.signature,
            }
        };

        let builder = {
            dao_contract::vote::wallet::Builder {
                inputs: vec![input],
                vote: dao_contract::vote::wallet::Vote {
                    vote_option,
                    vote_option_blind: pallas::Scalar::random(&mut OsRng),
                },
                vote_keypair: self.keypair,
                proposal: proposal.clone(),
                dao: dao_params.clone(),
            }
        };
        let func_call = builder.build(zk_bins);
        let func_calls = vec![func_call];

        let signatures = sign(vec![self.signature], &func_calls);
        Ok(Transaction { func_calls, signatures })
    }
}

pub struct DaoDemo {}

impl DaoDemo {
    pub fn new() -> Self {
        Self {}
    }

    fn init(&mut self) -> Result<()> {
        Ok(())
    }

    fn create(&mut self) -> Result<()> {
        Ok(())
    }

    fn mint(&mut self) -> Result<()> {
        Ok(())
    }

    fn airdrop(&mut self) -> Result<()> {
        Ok(())
    }

    fn propose(&mut self) -> Result<()> {
        Ok(())
    }

    fn vote(&mut self) -> Result<()> {
        Ok(())
    }

    fn exec(&mut self) -> Result<()> {
        Ok(())
    }
}

async fn start() -> Result<()> {
    // daod
    //
    let rpc_addr = Url::parse("tcp://127.0.0.1:7777")?;
    let mut demo = DaoDemo::new();
    /////////////////////////////////////////////////
    //// init()
    /////////////////////////////////////////////////
    demo.init()?;
    let client = JsonRpcInterface::new(demo);

    let rpc_interface = Arc::new(client);

    listen_and_serve(rpc_addr, rpc_interface).await?;
    Ok(())
}

#[async_std::main]
async fn main() -> Result<()> {
    TermLogger::init(
        LevelFilter::Debug,
        simplelog::Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )?;

    start().await?;
    // demo().await?;
    Ok(())
}
