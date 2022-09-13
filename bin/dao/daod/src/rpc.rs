use std::{any::TypeId, sync::Arc, time::Instant};

use async_std::sync::Mutex;
use async_trait::async_trait;
use incrementalmerkletree::{Position, Tree};
use log::debug;
use pasta_curves::{
    arithmetic::CurveAffine,
    group::{ff::Field, Curve, Group},
    pallas, Fp, Fq,
};
use rand::rngs::OsRng;
use serde_json::{json, Value};

use darkfi::{
    crypto::{
        keypair::{Keypair, PublicKey, SecretKey},
        proof::{ProvingKey, VerifyingKey},
        types::{DrkSpendHook, DrkUserData, DrkValue},
        util::{pedersen_commitment_u64, poseidon_hash},
    },
    rpc::{
        jsonrpc::{ErrorCode::*, JsonError, JsonRequest, JsonResponse, JsonResult},
        server::RequestHandler,
    },
    zk::circuit::{BurnContract, MintContract},
    zkas::decoder::ZkBinary,
    Result,
};

use crate::{
    contract::{
        self,
        dao_contract::{self, mint::wallet::DaoParams, propose::wallet::Proposal, DaoBulla},
        money_contract::{self, state::OwnCoin, transfer::Note},
    },
    util::{sign, StateRegistry, Transaction, ZkContractTable},
};

pub struct GloVar {
    dao_keypair: Keypair,
    dao_bulla: DaoBulla,
    dao_leaf_position: Position,
    dao_bulla_blind: Fp,
    cashier_signature_secret: SecretKey,
    xdrk_token_id: Fp,
    gdrk_token_id: Fp,
    gov_recv: Vec<OwnCoin>,
    gov_keypairs: Vec<Keypair>,
    proposal: Proposal,
    dao_params: DaoParams,
    treasury_note: Note,
    dao_recv_coin: OwnCoin,
    user_keypair: Keypair,
    proposal_bulla: Fp,
    yes_votes_value: u64,
    all_votes_value: u64,
    yes_votes_blind: Fq,
    all_votes_blind: Fq,
}

impl GloVar {
    pub fn new() -> Self {
        let dao_keypair = Keypair::random(&mut OsRng);
        let dao_bulla = pallas::Base::random(&mut OsRng);
        let dao_bulla_blind = pallas::Base::random(&mut OsRng);
        let dao_leaf_position = Position::zero();
        let xdrk_token_id = pallas::Base::random(&mut OsRng);
        let gdrk_token_id = pallas::Base::random(&mut OsRng);
        let cashier_signature_secret = SecretKey::random(&mut OsRng);
        let gov_recv = vec![];
        let gov_keypairs = vec![];
        // randomly filled
        let proposal = dao_contract::propose::wallet::Proposal {
            dest: PublicKey::random(&mut OsRng),
            amount: 1000,
            serial: pallas::Base::random(&mut OsRng),
            token_id: xdrk_token_id,
            blind: pallas::Base::random(&mut OsRng),
        };
        // randomly filled
        let dao_params = dao_contract::mint::wallet::DaoParams {
            proposer_limit: 0,
            quorum: 0,
            approval_ratio: 0,
            gov_token_id: gdrk_token_id,
            public_key: dao_keypair.public,
            bulla_blind: dao_bulla_blind,
        };
        // randomly filled
        let treasury_note = Note {
            serial: Fp::zero(),
            value: 0,
            token_id: Fp::zero(),
            spend_hook: Fp::zero(),
            user_data: Fp::zero(),
            coin_blind: Fp::zero(),
            value_blind: Fq::zero(),
            token_blind: Fq::zero(),
        };
        let dao_recv_coin = OwnCoin {
            coin: darkfi::crypto::coin::Coin(pallas::Base::zero()),
            note: treasury_note.clone(),
            leaf_position: Position::zero(),
        };
        let user_keypair = Keypair::random(&mut OsRng);
        let proposal_bulla = pallas::Base::random(&mut OsRng);
        let yes_votes_value = 0;
        let all_votes_value = 0;
        let yes_votes_blind = Fq::zero();
        let all_votes_blind = Fq::zero();
        Self {
            dao_keypair,
            dao_bulla: contract::dao_contract::state::DaoBulla(dao_bulla),
            dao_bulla_blind,
            dao_leaf_position,
            xdrk_token_id,
            gdrk_token_id,
            cashier_signature_secret,
            gov_recv,
            gov_keypairs,
            proposal,
            dao_params,
            treasury_note,
            dao_recv_coin,
            user_keypair,
            proposal_bulla,
            yes_votes_value,
            all_votes_value,
            yes_votes_blind,
            all_votes_blind,
        }
    }
}

pub struct JsonRpcInterface {
    states: Arc<Mutex<StateRegistry>>,
    zk_bins: Arc<Mutex<ZkContractTable>>,
    global_var: Arc<Mutex<GloVar>>,
}

#[async_trait]
impl RequestHandler for JsonRpcInterface {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        if !req.params.is_array() {
            return JsonError::new(InvalidParams, None, req.id).into()
        }

        let params = req.params.as_array().unwrap();

        debug!(target: "RPC", "--> {}", serde_json::to_string(&req).unwrap());

        match req.method.as_str() {
            Some("create") => return self.create_dao(req.id, params).await,
            Some("mint") => return self.mint_tokens(req.id, params).await,
            Some("airdrop") => return self.airdrop_tokens(req.id, params).await,
            Some("propose") => return self.create_proposal(req.id, params).await,
            Some("vote") => return self.vote(req.id, params).await,
            Some("exec") => return self.execute(req.id, params).await,
            Some(_) | None => return JsonError::new(MethodNotFound, None, req.id).into(),
        }
    }
}

impl JsonRpcInterface {
    pub fn new() -> Self {
        let states = Arc::new(Mutex::new(StateRegistry::new()));
        let zk_bins = Arc::new(Mutex::new(ZkContractTable::new()));
        let global_var = Arc::new(Mutex::new(GloVar::new()));
        Self { states, zk_bins, global_var }
    }
    pub async fn init(&self) -> Result<()> {
        let mut zk_bins = self.zk_bins.lock().await;

        debug!(target: "demo", "Loading dao-mint.zk");
        let zk_dao_mint_bincode = include_bytes!("../proof/dao-mint.zk.bin");
        let zk_dao_mint_bin = ZkBinary::decode(zk_dao_mint_bincode)?;
        zk_bins.add_contract("dao-mint".to_string(), zk_dao_mint_bin, 13);

        debug!(target: "demo", "Loading money-transfer contracts");
        {
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

            zk_bins.add_native("money-transfer-mint".to_string(), mint_pk, mint_vk);
            zk_bins.add_native("money-transfer-burn".to_string(), burn_pk, burn_vk);
        }
        debug!(target: "demo", "Loading dao-propose-main.zk");
        let zk_dao_propose_main_bincode = include_bytes!("../proof/dao-propose-main.zk.bin");
        let zk_dao_propose_main_bin = ZkBinary::decode(zk_dao_propose_main_bincode)?;
        zk_bins.add_contract("dao-propose-main".to_string(), zk_dao_propose_main_bin, 13);
        debug!(target: "demo", "Loading dao-propose-burn.zk");
        let zk_dao_propose_burn_bincode = include_bytes!("../proof/dao-propose-burn.zk.bin");
        let zk_dao_propose_burn_bin = ZkBinary::decode(zk_dao_propose_burn_bincode)?;
        zk_bins.add_contract("dao-propose-burn".to_string(), zk_dao_propose_burn_bin, 13);
        debug!(target: "demo", "Loading dao-vote-main.zk");
        let zk_dao_vote_main_bincode = include_bytes!("../proof/dao-vote-main.zk.bin");
        let zk_dao_vote_main_bin = ZkBinary::decode(zk_dao_vote_main_bincode)?;
        zk_bins.add_contract("dao-vote-main".to_string(), zk_dao_vote_main_bin, 13);
        debug!(target: "demo", "Loading dao-vote-burn.zk");
        let zk_dao_vote_burn_bincode = include_bytes!("../proof/dao-vote-burn.zk.bin");
        let zk_dao_vote_burn_bin = ZkBinary::decode(zk_dao_vote_burn_bincode)?;
        zk_bins.add_contract("dao-vote-burn".to_string(), zk_dao_vote_burn_bin, 13);
        let zk_dao_exec_bincode = include_bytes!("../proof/dao-exec.zk.bin");
        let zk_dao_exec_bin = ZkBinary::decode(zk_dao_exec_bincode)?;
        zk_bins.add_contract("dao-exec".to_string(), zk_dao_exec_bin, 13);
        drop(zk_bins);

        // State for money contracts
        let cashier_signature_secret = self.global_var.lock().await.cashier_signature_secret;
        let cashier_signature_public = PublicKey::from_secret(cashier_signature_secret);
        let faucet_signature_secret = SecretKey::random(&mut OsRng);
        let faucet_signature_public = PublicKey::from_secret(faucet_signature_secret);

        ///////////////////////////////////////////////////
        {
            let mut states = self.states.lock().await;
            let money_state = money_contract::state::State::new(
                cashier_signature_public,
                faucet_signature_public,
            );
            states.register(*money_contract::CONTRACT_ID, money_state);
        }

        /////////////////////////////////////////////////////

        {
            let mut states = self.states.lock().await;
            let dao_state = dao_contract::State::new();
            states.register(*dao_contract::CONTRACT_ID, dao_state);
        }

        /////////////////////////////////////////////////////

        Ok(())
    }
    // --> {"method": "create", "params": []}
    // <-- {"result": "creating dao..."}
    async fn create_dao(&self, id: Value, _params: &[Value]) -> JsonResult {
        self.create().await.unwrap();
        JsonResponse::new(json!("dao created"), id).into()
    }
    // --> {"method": "mint_tokens", "params": []}
    // <-- {"result": "minting tokens..."}
    async fn mint_tokens(&self, id: Value, _params: &[Value]) -> JsonResult {
        self.mint().await.unwrap();
        JsonResponse::new(json!("tokens minted"), id).into()
    }
    // --> {"method": "airdrop_tokens", "params": []}
    // <-- {"result": "airdropping tokens..."}
    async fn airdrop_tokens(&self, id: Value, _params: &[Value]) -> JsonResult {
        self.airdrop().await.unwrap();
        JsonResponse::new(json!("tokens airdropped"), id).into()
    }
    // --> {"method": "create_proposal", "params": []}
    // <-- {"result": "creating proposal..."}
    async fn create_proposal(&self, id: Value, _params: &[Value]) -> JsonResult {
        self.propose().await.unwrap();
        JsonResponse::new(json!("proposal created"), id).into()
    }
    // --> {"method": "vote", "params": []}
    // <-- {"result": "voting..."}
    async fn vote(&self, id: Value, _params: &[Value]) -> JsonResult {
        self.voting().await.unwrap();
        JsonResponse::new(json!("voted"), id).into()
    }
    // --> {"method": "execute", "params": []}
    // <-- {"result": "executing..."}
    async fn execute(&self, id: Value, _params: &[Value]) -> JsonResult {
        self.exec().await.unwrap();
        JsonResponse::new(json!("executed"), id).into()
    }

    async fn create(&self) -> Result<()> {
        /////////////////////////////////////////////////
        //// create()
        /////////////////////////////////////////////////

        /////////////////////////////////////////////////////
        ////// Create the DAO bulla
        /////////////////////////////////////////////////////

        // DAO parameters
        let dao_proposer_limit = 110;
        let dao_quorum = 110;
        let dao_approval_ratio = 2;

        debug!(target: "demo", "Stage 1. Creating DAO bulla");

        //// Wallet

        //// Setup the DAO
        let dao_keypair = Keypair::random(&mut OsRng);
        let dao_bulla_blind = pallas::Base::random(&mut OsRng);

        let signature_secret = SecretKey::random(&mut OsRng);
        // Create DAO mint tx
        let builder = dao_contract::mint::wallet::Builder {
            dao_proposer_limit,
            dao_quorum,
            dao_approval_ratio,
            gov_token_id: self.global_var.lock().await.gdrk_token_id,
            dao_pubkey: dao_keypair.public,
            dao_bulla_blind,
            _signature_secret: signature_secret,
        };
        let func_call = builder.build(&*self.zk_bins.lock().await);
        let func_calls = vec![func_call];

        let signatures = sign(vec![signature_secret], &func_calls);
        let tx = Transaction { func_calls, signatures };

        //// Validator

        let mut updates = vec![];
        {
            let states = self.states.lock().await;

            // Validate all function calls in the tx
            for (idx, func_call) in tx.func_calls.iter().enumerate() {
                // So then the verifier will lookup the corresponding state_transition and apply
                // functions based off the func_id
                if func_call.func_id == *dao_contract::mint::FUNC_ID {
                    debug!("dao_contract::mint::state_transition()");

                    let update = dao_contract::mint::validate::state_transition(&states, idx, &tx)
                        .expect("dao_contract::mint::validate::state_transition() failed!");
                    updates.push(update);
                }
            }
        }

        {
            let mut states = self.states.lock().await;
            // Atomically apply all changes
            for update in updates {
                update.apply(&mut states);
            }
        }

        tx.zk_verify(&*self.zk_bins.lock().await);
        tx.verify_sigs();

        // Wallet stuff

        // In your wallet, wait until you see the tx confirmed before doing anything below
        // So for example keep track of tx hash
        //assert_eq!(tx.hash(), tx_hash);

        // We need to witness() the value in our local merkle tree
        // Must be called as soon as this DAO bulla is added to the state
        let dao_leaf_position = {
            let mut states = self.states.lock().await;
            let state =
                states.lookup_mut::<dao_contract::State>(*dao_contract::CONTRACT_ID).unwrap();
            state.dao_tree.witness().unwrap()
        };

        // It might just be easier to hash it ourselves from keypair and blind...
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

        {
            let mut glovar = self.global_var.lock().await;
            glovar.dao_bulla = dao_bulla;
            glovar.dao_keypair = dao_keypair;
            glovar.dao_leaf_position = dao_leaf_position;
            glovar.dao_bulla_blind = dao_bulla_blind;
        }

        Ok(())
    }

    async fn mint(&self) -> Result<()> {
        /////////////////////////////////////////////////
        //// mint()
        /////////////////////////////////////////////////

        ///////////////////////////////////////////////////
        //// Mint the initial supply of treasury token
        //// and send it all to the DAO directly
        ///////////////////////////////////////////////////

        // Money parameters
        let xdrk_supply = 1_000_000;

        debug!(target: "demo", "Stage 2. Minting treasury token");
        {
            let mut states = self.states.lock().await;
            let state =
                states.lookup_mut::<money_contract::State>(*money_contract::CONTRACT_ID).unwrap();
            state.wallet_cache.track(self.global_var.lock().await.dao_keypair.secret);
        }
        //// Wallet

        // Address of deployed contract in our example is dao_contract::exec::FUNC_ID
        // This field is public, you can see it's being sent to a DAO
        // but nothing else is visible.
        //
        // In the python code we wrote:
        //
        //   spend_hook = b"0xdao_ruleset"
        //
        let spend_hook = *dao_contract::exec::FUNC_ID;
        // The user_data can be a simple hash of the items passed into the ZK proof
        // up to corresponding linked ZK proof to interpret however they need.
        // In out case, it's the bulla for the DAO
        let user_data = self.global_var.lock().await.dao_bulla.0;
        let builder = {
            let glovar = self.global_var.lock().await;
            money_contract::transfer::wallet::Builder {
                clear_inputs: vec![money_contract::transfer::wallet::BuilderClearInputInfo {
                    value: xdrk_supply,
                    token_id: glovar.xdrk_token_id,
                    signature_secret: glovar.cashier_signature_secret,
                }],
                inputs: vec![],
                outputs: vec![money_contract::transfer::wallet::BuilderOutputInfo {
                    value: xdrk_supply,
                    token_id: glovar.xdrk_token_id,
                    public: glovar.dao_keypair.public,
                    serial: pallas::Base::random(&mut OsRng),
                    coin_blind: pallas::Base::random(&mut OsRng),
                    spend_hook,
                    user_data,
                }],
            }
        };
        let func_call = builder.build(&*self.zk_bins.lock().await)?;
        let func_calls = vec![func_call];

        let signatures =
            sign(vec![self.global_var.lock().await.cashier_signature_secret], &func_calls);
        let tx = Transaction { func_calls, signatures };

        //// Validator
        let mut updates = vec![];
        {
            let states = &*self.states.lock().await;
            // Validate all function calls in the tx
            for (idx, func_call) in tx.func_calls.iter().enumerate() {
                // So then the verifier will lookup the corresponding state_transition and apply
                // functions based off the func_id
                if func_call.func_id == *money_contract::transfer::FUNC_ID {
                    debug!("money_contract::transfer::state_transition()");

                    let update =
                        money_contract::transfer::validate::state_transition(states, idx, &tx)
                            .expect(
                                "money_contract::transfer::validate::state_transition() failed!",
                            );
                    updates.push(update);
                }
            }
        }
        {
            let mut states = self.states.lock().await;
            // Atomically apply all changes
            for update in updates {
                update.apply(&mut states);
            }
        }

        tx.zk_verify(&*self.zk_bins.lock().await);
        tx.verify_sigs();

        //// Wallet
        // DAO reads the money received from the encrypted note

        let mut states = self.states.lock().await;
        let state =
            states.lookup_mut::<money_contract::State>(*money_contract::CONTRACT_ID).unwrap();

        let mut recv_coins =
            state.wallet_cache.get_received(&self.global_var.lock().await.dao_keypair.secret);
        assert_eq!(recv_coins.len(), 1);
        let dao_recv_coin = recv_coins.pop().unwrap();
        let treasury_note = dao_recv_coin.note.clone();
        drop(states);
        // Check the actual coin received is valid before accepting it

        let coords =
            self.global_var.lock().await.dao_keypair.public.0.to_affine().coordinates().unwrap();
        let coin = poseidon_hash::<8>([
            *coords.x(),
            *coords.y(),
            DrkValue::from(treasury_note.value),
            treasury_note.token_id,
            treasury_note.serial,
            treasury_note.spend_hook,
            treasury_note.user_data,
            treasury_note.coin_blind,
        ]);
        assert_eq!(coin, dao_recv_coin.coin.0);

        assert_eq!(treasury_note.spend_hook, *dao_contract::exec::FUNC_ID);
        assert_eq!(treasury_note.user_data, self.global_var.lock().await.dao_bulla.0);

        debug!("DAO received a coin worth {} xDRK", treasury_note.value);

        {
            let mut glovar = self.global_var.lock().await;
            glovar.treasury_note = treasury_note;
            glovar.dao_recv_coin = dao_recv_coin;
        }

        Ok(())
    }

    async fn airdrop(&self) -> Result<()> {
        /////////////////////////////////////////////////
        //// airdrop()
        /////////////////////////////////////////////////

        ///////////////////////////////////////////////////
        //// Mint the governance token
        //// Send it to three hodlers
        ///////////////////////////////////////////////////

        // Governance token parameters
        let gdrk_supply = 1_000_000;

        debug!(target: "demo", "Stage 3. Minting governance token");

        //// Wallet

        // Hodler 1
        let gov_keypair_1 = Keypair::random(&mut OsRng);
        // Hodler 2
        let gov_keypair_2 = Keypair::random(&mut OsRng);
        // Hodler 3: the tiebreaker
        let gov_keypair_3 = Keypair::random(&mut OsRng);
        {
            let mut states = self.states.lock().await;
            let state =
                states.lookup_mut::<money_contract::State>(*money_contract::CONTRACT_ID).unwrap();
            state.wallet_cache.track(gov_keypair_1.secret);
            state.wallet_cache.track(gov_keypair_2.secret);
            state.wallet_cache.track(gov_keypair_3.secret);
        }

        let gov_keypairs = vec![gov_keypair_1, gov_keypair_2, gov_keypair_3];

        // Spend hook and user data disabled
        let spend_hook = DrkSpendHook::from(0);
        let user_data = DrkUserData::from(0);

        let output1 = money_contract::transfer::wallet::BuilderOutputInfo {
            value: 400000,
            token_id: self.global_var.lock().await.gdrk_token_id,
            public: gov_keypairs[0].public,
            serial: pallas::Base::random(&mut OsRng),
            coin_blind: pallas::Base::random(&mut OsRng),
            spend_hook,
            user_data,
        };

        let output2 = money_contract::transfer::wallet::BuilderOutputInfo {
            value: 400000,
            token_id: self.global_var.lock().await.gdrk_token_id,
            public: gov_keypairs[1].public,
            serial: pallas::Base::random(&mut OsRng),
            coin_blind: pallas::Base::random(&mut OsRng),
            spend_hook,
            user_data,
        };

        let output3 = money_contract::transfer::wallet::BuilderOutputInfo {
            value: 200000,
            token_id: self.global_var.lock().await.gdrk_token_id,
            public: gov_keypairs[2].public,
            serial: pallas::Base::random(&mut OsRng),
            coin_blind: pallas::Base::random(&mut OsRng),
            spend_hook,
            user_data,
        };

        assert!(2 * 400000 + 200000 == gdrk_supply);

        let builder = {
            let glovar = self.global_var.lock().await;
            money_contract::transfer::wallet::Builder {
                clear_inputs: vec![money_contract::transfer::wallet::BuilderClearInputInfo {
                    value: gdrk_supply,
                    token_id: glovar.gdrk_token_id,
                    signature_secret: glovar.cashier_signature_secret,
                }],
                inputs: vec![],
                outputs: vec![output1, output2, output3],
            }
        };

        let func_call = builder.build(&*self.zk_bins.lock().await)?;
        let func_calls = vec![func_call];

        let signatures =
            sign(vec![self.global_var.lock().await.cashier_signature_secret], &func_calls);
        let tx = Transaction { func_calls, signatures };

        //// Validator

        let mut updates = vec![];
        {
            let states = &*self.states.lock().await;
            // Validate all function calls in the tx
            for (idx, func_call) in tx.func_calls.iter().enumerate() {
                // So then the verifier will lookup the corresponding state_transition and apply
                // functions based off the func_id
                if func_call.func_id == *money_contract::transfer::FUNC_ID {
                    debug!("money_contract::transfer::state_transition()");

                    let update =
                        money_contract::transfer::validate::state_transition(states, idx, &tx)
                            .expect(
                                "money_contract::transfer::validate::state_transition() failed!",
                            );
                    updates.push(update);
                }
            }
        }

        {
            let mut states = self.states.lock().await;
            // Atomically apply all changes
            for update in updates {
                update.apply(&mut states);
            }
        }

        tx.zk_verify(&*self.zk_bins.lock().await);
        tx.verify_sigs();

        //// Wallet

        let mut gov_recv = vec![None, None, None];
        {
            let mut states = self.states.lock().await;
            let gdrk_token_id = self.global_var.lock().await.gdrk_token_id;
            // Check that each person received one coin
            for (i, key) in gov_keypairs.iter().enumerate() {
                let gov_recv_coin = {
                    let state = states
                        .lookup_mut::<money_contract::State>(*money_contract::CONTRACT_ID)
                        .unwrap();
                    let mut recv_coins = state.wallet_cache.get_received(&key.secret);
                    assert_eq!(recv_coins.len(), 1);
                    let recv_coin = recv_coins.pop().unwrap();
                    let note = &recv_coin.note;

                    assert_eq!(note.token_id, gdrk_token_id);
                    // Normal payment
                    assert_eq!(note.spend_hook, pallas::Base::from(0));
                    assert_eq!(note.user_data, pallas::Base::from(0));

                    let coords = key.public.0.to_affine().coordinates().unwrap();
                    let coin = poseidon_hash::<8>([
                        *coords.x(),
                        *coords.y(),
                        DrkValue::from(note.value),
                        note.token_id,
                        note.serial,
                        note.spend_hook,
                        note.user_data,
                        note.coin_blind,
                    ]);
                    assert_eq!(coin, recv_coin.coin.0);

                    debug!("Holder{} received a coin worth {} gDRK", i, note.value);

                    recv_coin
                };
                gov_recv[i] = Some(gov_recv_coin);
            }
        }
        // unwrap them for this demo
        let gov_recv: Vec<_> = gov_recv.into_iter().map(|r| r.unwrap()).collect();

        {
            let mut glovar = self.global_var.lock().await;
            glovar.gov_recv = gov_recv;
            glovar.gov_keypairs = gov_keypairs;
        }

        Ok(())
    }

    async fn propose(&self) -> Result<()> {
        ///////////////////////////////////////////////////
        // DAO rules:
        // 1. gov token IDs must match on all inputs
        // 2. proposals must be submitted by minimum amount
        // 3. all votes >= quorum
        // 4. outcome > approval_ratio
        // 5. structure of outputs
        //   output 0: value and address
        //   output 1: change address
        ///////////////////////////////////////////////////

        /////////////////////////////////////////////////
        //// propose()
        /////////////////////////////////////////////////

        ///////////////////////////////////////////////////
        // Propose the vote
        // In order to make a valid vote, first the proposer must
        // meet a criteria for a minimum number of gov tokens
        ///////////////////////////////////////////////////

        // DAO parameters
        let dao_proposer_limit = 110;
        let dao_quorum = 110;
        let dao_approval_ratio = 2;

        debug!(target: "demo", "Stage 4. Propose the vote");

        //// Wallet

        // TODO: look into proposal expiry once time for voting has finished

        let (money_leaf_position, money_merkle_path) = {
            let states = self.states.lock().await;
            let state =
                states.lookup::<money_contract::State>(*money_contract::CONTRACT_ID).unwrap();
            let tree = &state.tree;
            let leaf_position = self.global_var.lock().await.gov_recv[0].leaf_position;
            let root = tree.root(0).unwrap();
            let merkle_path = tree.authentication_path(leaf_position, &root).unwrap();
            (leaf_position, merkle_path)
        };

        // TODO: is it possible for an invalid transfer() to be constructed on exec()?
        //       need to look into this
        let signature_secret = SecretKey::random(&mut OsRng);
        let input = {
            let glovar = self.global_var.lock().await;
            dao_contract::propose::wallet::BuilderInput {
                secret: glovar.gov_keypairs[0].secret,
                note: glovar.gov_recv[0].note.clone(),
                leaf_position: money_leaf_position,
                merkle_path: money_merkle_path,
                signature_secret,
            }
        };

        let (dao_merkle_path, dao_merkle_root) = {
            let states = self.states.lock().await;
            let state = states.lookup::<dao_contract::State>(*dao_contract::CONTRACT_ID).unwrap();
            let tree = &state.dao_tree;
            let root = tree.root(0).unwrap();
            let merkle_path = tree
                .authentication_path(self.global_var.lock().await.dao_leaf_position, &root)
                .unwrap();
            (merkle_path, root)
        };

        let dao_params = {
            let glovar = self.global_var.lock().await;
            dao_contract::mint::wallet::DaoParams {
                proposer_limit: dao_proposer_limit,
                quorum: dao_quorum,
                approval_ratio: dao_approval_ratio,
                gov_token_id: glovar.gdrk_token_id,
                public_key: glovar.dao_keypair.public,
                bulla_blind: glovar.dao_bulla_blind,
            }
        };

        let proposal = {
            let glovar = self.global_var.lock().await;
            dao_contract::propose::wallet::Proposal {
                dest: glovar.user_keypair.public,
                amount: 1000,
                serial: pallas::Base::random(&mut OsRng),
                token_id: glovar.xdrk_token_id,
                blind: pallas::Base::random(&mut OsRng),
            }
        };

        let builder = dao_contract::propose::wallet::Builder {
            inputs: vec![input],
            proposal: proposal.clone(),
            dao: dao_params.clone(),
            dao_leaf_position: self.global_var.lock().await.dao_leaf_position,
            dao_merkle_path,
            dao_merkle_root,
        };

        let func_call = builder.build(&*self.zk_bins.lock().await);
        let func_calls = vec![func_call];

        let signatures = sign(vec![signature_secret], &func_calls);
        let tx = Transaction { func_calls, signatures };

        //// Validator

        let mut updates = vec![];
        {
            let states = self.states.lock().await;
            // Validate all function calls in the tx
            for (idx, func_call) in tx.func_calls.iter().enumerate() {
                if func_call.func_id == *dao_contract::propose::FUNC_ID {
                    debug!(target: "demo", "dao_contract::propose::state_transition()");

                    let update =
                        dao_contract::propose::validate::state_transition(&states, idx, &tx)
                            .expect("dao_contract::propose::validate::state_transition() failed!");
                    updates.push(update);
                }
            }
        }

        {
            let mut states = self.states.lock().await;
            // Atomically apply all changes
            for update in updates {
                update.apply(&mut states);
            }
        }

        tx.zk_verify(&*self.zk_bins.lock().await);
        tx.verify_sigs();

        //// Wallet

        // Read received proposal
        let (proposal, proposal_bulla) = {
            let glovar = self.global_var.lock().await;
            assert_eq!(tx.func_calls.len(), 1);
            let func_call = &tx.func_calls[0];
            let call_data = func_call.call_data.as_any();
            assert_eq!(
                (&*call_data).type_id(),
                TypeId::of::<dao_contract::propose::validate::CallData>()
            );
            let call_data =
                call_data.downcast_ref::<dao_contract::propose::validate::CallData>().unwrap();

            let header = &call_data.header;
            let note: dao_contract::propose::wallet::Note =
                header.enc_note.decrypt(&glovar.dao_keypair.secret).unwrap();

            // TODO: check it belongs to DAO bulla

            // Return the proposal info
            (note.proposal, call_data.header.proposal_bulla)
        };
        debug!(target: "demo", "Proposal now active!");
        debug!(target: "demo", "  destination: {:?}", proposal.dest);
        debug!(target: "demo", "  amount: {}", proposal.amount);
        debug!(target: "demo", "  token_id: {:?}", proposal.token_id);
        debug!(target: "demo", "  dao_bulla: {:?}", self.global_var.lock().await.dao_bulla.0);
        debug!(target: "demo", "Proposal bulla: {:?}", proposal_bulla);

        {
            let mut glovar = self.global_var.lock().await;
            glovar.proposal = proposal;
            glovar.dao_params = dao_params;
            glovar.proposal_bulla = proposal_bulla;
        }

        Ok(())
    }

    async fn voting(&self) -> Result<()> {
        ///////////////////////////////////////////////////
        // Proposal is accepted!
        // Start the voting
        ///////////////////////////////////////////////////

        // Copying these schizo comments from python code:
        // Lets the voting begin
        // Voters have access to the proposal and dao data
        //   vote_state = VoteState()
        // We don't need to copy nullifier set because it is checked from gov_state
        // in vote_state_transition() anyway
        //
        // TODO: what happens if voters don't unblind their vote
        // Answer:
        //   1. there is a time limit
        //   2. both the MPC or users can unblind
        //
        // TODO: bug if I vote then send money, then we can double vote
        // TODO: all timestamps missing
        //       - timelock (future voting starts in 2 days)
        // Fix: use nullifiers from money gov state only from
        // beginning of gov period
        // Cannot use nullifiers from before voting period

        /////////////////////////////////////////////////
        //// vote()
        /////////////////////////////////////////////////

        debug!(target: "demo", "Stage 5. Start voting");

        // We were previously saving updates here for testing
        // let mut updates = vec![];

        // User 1: YES

        let (money_leaf_position, money_merkle_path) = {
            let states = self.states.lock().await;
            let state =
                states.lookup::<money_contract::State>(*money_contract::CONTRACT_ID).unwrap();
            let tree = &state.tree;
            let leaf_position = self.global_var.lock().await.gov_recv[0].leaf_position;
            let root = tree.root(0).unwrap();
            let merkle_path = tree.authentication_path(leaf_position, &root).unwrap();
            (leaf_position, merkle_path)
        };

        let signature_secret = SecretKey::random(&mut OsRng);
        let input = {
            let glovar = self.global_var.lock().await;
            dao_contract::vote::wallet::BuilderInput {
                secret: glovar.gov_keypairs[0].secret,
                note: glovar.gov_recv[0].note.clone(),
                leaf_position: money_leaf_position,
                merkle_path: money_merkle_path,
                signature_secret,
            }
        };

        let vote_option: bool = true;

        assert!(vote_option == true || vote_option == false);

        // We create a new keypair to encrypt the vote.
        // For the demo MVP, you can just use the dao_keypair secret
        let vote_keypair_1 = Keypair::random(&mut OsRng);

        let builder = {
            let glovar = self.global_var.lock().await;
            dao_contract::vote::wallet::Builder {
                inputs: vec![input],
                vote: dao_contract::vote::wallet::Vote {
                    vote_option,
                    vote_option_blind: pallas::Scalar::random(&mut OsRng),
                },
                vote_keypair: vote_keypair_1,
                proposal: glovar.proposal.clone(),
                dao: glovar.dao_params.clone(),
            }
        };
        debug!(target: "demo", "build()...");
        let func_call = builder.build(&*self.zk_bins.lock().await);
        let func_calls = vec![func_call];

        let signatures = sign(vec![signature_secret], &func_calls);
        let tx = Transaction { func_calls, signatures };

        //// Validator

        let mut updates = vec![];
        {
            let states = self.states.lock().await;
            // Validate all function calls in the tx
            for (idx, func_call) in tx.func_calls.iter().enumerate() {
                if func_call.func_id == *dao_contract::vote::FUNC_ID {
                    debug!(target: "demo", "dao_contract::vote::state_transition()");

                    let update = dao_contract::vote::validate::state_transition(&states, idx, &tx)
                        .expect("dao_contract::vote::validate::state_transition() failed!");
                    updates.push(update);
                }
            }
        }

        {
            let mut states = self.states.lock().await;
            // Atomically apply all changes
            for update in updates {
                update.apply(&mut states);
            }
        }

        tx.zk_verify(&*self.zk_bins.lock().await);
        tx.verify_sigs();

        //// Wallet

        // Secret vote info. Needs to be revealed at some point.
        // TODO: look into verifiable encryption for notes
        // TODO: look into timelock puzzle as a possibility
        let vote_note_1 = {
            assert_eq!(tx.func_calls.len(), 1);
            let func_call = &tx.func_calls[0];
            let call_data = func_call.call_data.as_any();
            assert_eq!(
                (&*call_data).type_id(),
                TypeId::of::<dao_contract::vote::validate::CallData>()
            );
            let call_data =
                call_data.downcast_ref::<dao_contract::vote::validate::CallData>().unwrap();

            let header = &call_data.header;
            let note: dao_contract::vote::wallet::Note =
                header.enc_note.decrypt(&vote_keypair_1.secret).unwrap();
            note
        };
        debug!(target: "demo", "User 1 voted!");
        debug!(target: "demo", "  vote_option: {}", vote_note_1.vote.vote_option);
        debug!(target: "demo", "  value: {}", vote_note_1.vote_value);

        // User 2: NO

        let (money_leaf_position, money_merkle_path) = {
            let states = self.states.lock().await;
            let state =
                states.lookup::<money_contract::State>(*money_contract::CONTRACT_ID).unwrap();
            let tree = &state.tree;
            let leaf_position = self.global_var.lock().await.gov_recv[1].leaf_position;
            let root = tree.root(0).unwrap();
            let merkle_path = tree.authentication_path(leaf_position, &root).unwrap();
            (leaf_position, merkle_path)
        };

        let signature_secret = SecretKey::random(&mut OsRng);
        let input = {
            let glovar = self.global_var.lock().await;
            dao_contract::vote::wallet::BuilderInput {
                secret: glovar.gov_keypairs[1].secret,
                note: glovar.gov_recv[1].note.clone(),
                leaf_position: money_leaf_position,
                merkle_path: money_merkle_path,
                signature_secret,
            }
        };

        let vote_option: bool = false;

        assert!(vote_option == true || vote_option == false);

        // We create a new keypair to encrypt the vote.
        let vote_keypair_2 = Keypair::random(&mut OsRng);

        let builder = {
            let glovar = self.global_var.lock().await;
            dao_contract::vote::wallet::Builder {
                inputs: vec![input],
                vote: dao_contract::vote::wallet::Vote {
                    vote_option,
                    vote_option_blind: pallas::Scalar::random(&mut OsRng),
                },
                vote_keypair: vote_keypair_2,
                proposal: glovar.proposal.clone(),
                dao: glovar.dao_params.clone(),
            }
        };
        debug!(target: "demo", "build()...");
        let func_call = builder.build(&*self.zk_bins.lock().await);
        let func_calls = vec![func_call];

        let signatures = sign(vec![signature_secret], &func_calls);
        let tx = Transaction { func_calls, signatures };

        //// Validator

        let mut updates = vec![];
        {
            let states = self.states.lock().await;
            // Validate all function calls in the tx
            for (idx, func_call) in tx.func_calls.iter().enumerate() {
                if func_call.func_id == *dao_contract::vote::FUNC_ID {
                    debug!(target: "demo", "dao_contract::vote::state_transition()");

                    let update = dao_contract::vote::validate::state_transition(&states, idx, &tx)
                        .expect("dao_contract::vote::validate::state_transition() failed!");
                    updates.push(update);
                }
            }
        }

        {
            let mut states = self.states.lock().await;
            // Atomically apply all changes
            for update in updates {
                update.apply(&mut states);
            }
        }

        tx.zk_verify(&*self.zk_bins.lock().await);
        tx.verify_sigs();

        //// Wallet

        // Secret vote info. Needs to be revealed at some point.
        // TODO: look into verifiable encryption for notes
        // TODO: look into timelock puzzle as a possibility
        let vote_note_2 = {
            assert_eq!(tx.func_calls.len(), 1);
            let func_call = &tx.func_calls[0];
            let call_data = func_call.call_data.as_any();
            assert_eq!(
                (&*call_data).type_id(),
                TypeId::of::<dao_contract::vote::validate::CallData>()
            );
            let call_data =
                call_data.downcast_ref::<dao_contract::vote::validate::CallData>().unwrap();

            let header = &call_data.header;
            let note: dao_contract::vote::wallet::Note =
                header.enc_note.decrypt(&vote_keypair_2.secret).unwrap();
            note
        };
        debug!(target: "demo", "User 2 voted!");
        debug!(target: "demo", "  vote_option: {}", vote_note_2.vote.vote_option);
        debug!(target: "demo", "  value: {}", vote_note_2.vote_value);

        // User 3: YES

        let (money_leaf_position, money_merkle_path) = {
            let states = self.states.lock().await;
            let state =
                states.lookup::<money_contract::State>(*money_contract::CONTRACT_ID).unwrap();
            let tree = &state.tree;
            let leaf_position = self.global_var.lock().await.gov_recv[2].leaf_position;
            let root = tree.root(0).unwrap();
            let merkle_path = tree.authentication_path(leaf_position, &root).unwrap();
            (leaf_position, merkle_path)
        };

        let signature_secret = SecretKey::random(&mut OsRng);
        let input = {
            let glovar = self.global_var.lock().await;
            dao_contract::vote::wallet::BuilderInput {
                secret: glovar.gov_keypairs[2].secret,
                note: glovar.gov_recv[2].note.clone(),
                leaf_position: money_leaf_position,
                merkle_path: money_merkle_path,
                signature_secret,
            }
        };

        let vote_option: bool = true;

        assert!(vote_option == true || vote_option == false);

        // We create a new keypair to encrypt the vote.
        let vote_keypair_3 = Keypair::random(&mut OsRng);

        let builder = {
            let glovar = self.global_var.lock().await;
            dao_contract::vote::wallet::Builder {
                inputs: vec![input],
                vote: dao_contract::vote::wallet::Vote {
                    vote_option,
                    vote_option_blind: pallas::Scalar::random(&mut OsRng),
                },
                vote_keypair: vote_keypair_3,
                proposal: glovar.proposal.clone(),
                dao: glovar.dao_params.clone(),
            }
        };
        debug!(target: "demo", "build()...");
        let func_call = builder.build(&*self.zk_bins.lock().await);
        let func_calls = vec![func_call];

        let signatures = sign(vec![signature_secret], &func_calls);
        let tx = Transaction { func_calls, signatures };

        //// Validator

        let mut updates = vec![];
        {
            let states = self.states.lock().await;
            // Validate all function calls in the tx
            for (idx, func_call) in tx.func_calls.iter().enumerate() {
                if func_call.func_id == *dao_contract::vote::FUNC_ID {
                    debug!(target: "demo", "dao_contract::vote::state_transition()");

                    let update = dao_contract::vote::validate::state_transition(&states, idx, &tx)
                        .expect("dao_contract::vote::validate::state_transition() failed!");
                    updates.push(update);
                }
            }
        }

        {
            let mut states = self.states.lock().await;
            // Atomically apply all changes
            for update in updates {
                update.apply(&mut states);
            }
        }

        tx.zk_verify(&*self.zk_bins.lock().await);
        tx.verify_sigs();

        //// Wallet

        // Secret vote info. Needs to be revealed at some point.
        // TODO: look into verifiable encryption for notes
        // TODO: look into timelock puzzle as a possibility
        let vote_note_3 = {
            assert_eq!(tx.func_calls.len(), 1);
            let func_call = &tx.func_calls[0];
            let call_data = func_call.call_data.as_any();
            assert_eq!(
                (&*call_data).type_id(),
                TypeId::of::<dao_contract::vote::validate::CallData>()
            );
            let call_data =
                call_data.downcast_ref::<dao_contract::vote::validate::CallData>().unwrap();

            let header = &call_data.header;
            let note: dao_contract::vote::wallet::Note =
                header.enc_note.decrypt(&vote_keypair_3.secret).unwrap();
            note
        };
        debug!(target: "demo", "User 3 voted!");
        debug!(target: "demo", "  vote_option: {}", vote_note_3.vote.vote_option);
        debug!(target: "demo", "  value: {}", vote_note_3.vote_value);

        // Every votes produces a semi-homomorphic encryption of their vote.
        // Which is either yes or no
        // We copy the state tree for the governance token so coins can be used
        // to vote on other proposals at the same time.
        // With their vote, they produce a ZK proof + nullifier
        // The votes are unblinded by MPC to a selected party at the end of the
        // voting period.
        // (that's if we want votes to be hidden during voting)

        let mut yes_votes_value = 0;
        let mut yes_votes_blind = pallas::Scalar::from(0);
        let mut yes_votes_commit = pallas::Point::identity();

        let mut all_votes_value = 0;
        let mut all_votes_blind = pallas::Scalar::from(0);
        let mut all_votes_commit = pallas::Point::identity();

        // We were previously saving votes to a Vec<Update> for testing.
        // However since Update is now UpdateBase it gets moved into update.apply().
        // So we need to think of another way to run these tests.
        //assert!(updates.len() == 3);

        for (i, note /* update*/) in [vote_note_1, vote_note_2, vote_note_3]
            .iter() /*.zip(updates)*/
            .enumerate()
        {
            let vote_commit = pedersen_commitment_u64(note.vote_value, note.vote_value_blind);
            //assert!(update.value_commit == all_vote_value_commit);
            all_votes_commit += vote_commit;
            all_votes_blind += note.vote_value_blind;

            let yes_vote_commit = pedersen_commitment_u64(
                note.vote.vote_option as u64 * note.vote_value,
                note.vote.vote_option_blind,
            );
            //assert!(update.yes_vote_commit == yes_vote_commit);

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

        {
            let mut glovar = self.global_var.lock().await;
            glovar.yes_votes_value = yes_votes_value;
            glovar.yes_votes_blind = yes_votes_blind;
            glovar.all_votes_value = all_votes_value;
            glovar.all_votes_blind = all_votes_blind;
        }
        Ok(())
    }

    async fn exec(&self) -> Result<()> {
        /////////////////////////////////////////////////
        //// exec()
        /////////////////////////////////////////////////

        ///////////////////////////////////////////////////
        // Execute the vote
        ///////////////////////////////////////////////////

        // Money parameters
        let xdrk_supply = 1_000_000;

        //// Wallet

        // Used to export user_data from this coin so it can be accessed by DAO::exec()
        let user_data_blind = pallas::Base::random(&mut OsRng);

        let user_serial = pallas::Base::random(&mut OsRng);
        let user_coin_blind = pallas::Base::random(&mut OsRng);
        let dao_serial = pallas::Base::random(&mut OsRng);
        let dao_coin_blind = pallas::Base::random(&mut OsRng);
        let input_value = self.global_var.lock().await.treasury_note.value;
        let input_value_blind = pallas::Scalar::random(&mut OsRng);
        let tx_signature_secret = SecretKey::random(&mut OsRng);
        let exec_signature_secret = SecretKey::random(&mut OsRng);

        let (treasury_leaf_position, treasury_merkle_path) = {
            let states = self.states.lock().await;
            let state =
                states.lookup::<money_contract::State>(*money_contract::CONTRACT_ID).unwrap();
            let tree = &state.tree;
            let leaf_position = self.global_var.lock().await.dao_recv_coin.leaf_position;
            let root = tree.root(0).unwrap();
            let merkle_path = tree.authentication_path(leaf_position, &root).unwrap();
            (leaf_position, merkle_path)
        };

        let input = {
            let glovar = self.global_var.lock().await;
            money_contract::transfer::wallet::BuilderInputInfo {
                leaf_position: treasury_leaf_position,
                merkle_path: treasury_merkle_path,
                secret: glovar.dao_keypair.secret,
                note: glovar.treasury_note.clone(),
                user_data_blind,
                value_blind: input_value_blind,
                signature_secret: tx_signature_secret,
            }
        };

        let builder = {
            let glovar = self.global_var.lock().await;
            money_contract::transfer::wallet::Builder {
                clear_inputs: vec![],
                inputs: vec![input],
                outputs: vec![
                    // Sending money
                    money_contract::transfer::wallet::BuilderOutputInfo {
                        value: 1000,
                        token_id: glovar.xdrk_token_id,
                        public: glovar.user_keypair.public,
                        serial: glovar.proposal.serial,
                        coin_blind: glovar.proposal.blind,
                        spend_hook: pallas::Base::from(0),
                        user_data: pallas::Base::from(0),
                    },
                    // Change back to DAO
                    money_contract::transfer::wallet::BuilderOutputInfo {
                        value: xdrk_supply - 1000,
                        token_id: glovar.xdrk_token_id,
                        public: glovar.dao_keypair.public,
                        serial: dao_serial,
                        coin_blind: dao_coin_blind,
                        spend_hook: *dao_contract::exec::FUNC_ID,
                        user_data: glovar.proposal_bulla,
                    },
                ],
            }
        };

        let transfer_func_call = builder.build(&*self.zk_bins.lock().await)?;

        let builder = {
            let glovar = self.global_var.lock().await;
            dao_contract::exec::wallet::Builder {
                proposal: glovar.proposal.clone(),
                dao: glovar.dao_params.clone(),
                yes_votes_value: glovar.yes_votes_value,
                all_votes_value: glovar.all_votes_value,
                yes_votes_blind: glovar.yes_votes_blind,
                all_votes_blind: glovar.all_votes_blind,
                user_serial,
                user_coin_blind,
                dao_serial,
                dao_coin_blind,
                input_value,
                input_value_blind,
                hook_dao_exec: *dao_contract::exec::FUNC_ID,
                signature_secret: exec_signature_secret,
            }
        };
        let exec_func_call = builder.build(&*self.zk_bins.lock().await);
        let func_calls = vec![transfer_func_call, exec_func_call];

        let signatures = sign(vec![tx_signature_secret, exec_signature_secret], &func_calls);
        let tx = Transaction { func_calls, signatures };

        {
            let glovar = self.global_var.lock().await;
            // Now the spend_hook field specifies the function DAO::exec()
            // so Money::transfer() must also be combined with DAO::exec()

            assert_eq!(tx.func_calls.len(), 2);
            let transfer_func_call = &tx.func_calls[0];
            let transfer_call_data = transfer_func_call.call_data.as_any();

            assert_eq!(
                (&*transfer_call_data).type_id(),
                TypeId::of::<money_contract::transfer::validate::CallData>()
            );
            let transfer_call_data =
                transfer_call_data.downcast_ref::<money_contract::transfer::validate::CallData>();
            let transfer_call_data = transfer_call_data.unwrap();
            // At least one input has this field value which means DAO::exec() is invoked.
            assert_eq!(transfer_call_data.inputs.len(), 1);
            let input = &transfer_call_data.inputs[0];
            assert_eq!(input.revealed.spend_hook, *dao_contract::exec::FUNC_ID);
            let user_data_enc = poseidon_hash::<2>([glovar.dao_bulla.0, user_data_blind]);
            assert_eq!(input.revealed.user_data_enc, user_data_enc);
        }

        //// Validator

        let mut updates = vec![];
        {
            let states = self.states.lock().await;
            // Validate all function calls in the tx
            for (idx, func_call) in tx.func_calls.iter().enumerate() {
                if func_call.func_id == *dao_contract::exec::FUNC_ID {
                    debug!("dao_contract::exec::state_transition()");

                    let update = dao_contract::exec::validate::state_transition(&states, idx, &tx)
                        .expect("dao_contract::exec::validate::state_transition() failed!");
                    updates.push(update);
                } else if func_call.func_id == *money_contract::transfer::FUNC_ID {
                    debug!("money_contract::transfer::state_transition()");

                    let update =
                        money_contract::transfer::validate::state_transition(&states, idx, &tx)
                            .expect(
                                "money_contract::transfer::validate::state_transition() failed!",
                            );
                    updates.push(update);
                }
            }
        }

        {
            let mut states = self.states.lock().await;
            // Atomically apply all changes
            for update in updates {
                update.apply(&mut states);
            }
        }

        // Other stuff
        tx.zk_verify(&*self.zk_bins.lock().await);
        tx.verify_sigs();

        //// Wallet

        Ok(())
    }
}
