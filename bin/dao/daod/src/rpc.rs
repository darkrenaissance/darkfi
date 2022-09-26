use std::sync::Arc;

use async_std::sync::Mutex;
use async_trait::async_trait;
use fxhash::FxHashMap;
use log::debug;
use pasta_curves::{group::ff::PrimeField, pallas};
use rand::rngs::OsRng;
use std::str::FromStr;

use serde_json::{json, Value};

use darkfi::{
    crypto::keypair::{Keypair, PublicKey, SecretKey},
    rpc::{
        jsonrpc::{ErrorCode::*, JsonError, JsonRequest, JsonResponse, JsonResult},
        server::RequestHandler,
    },
};

use crate::{
    contract::money_contract::state::OwnCoin,
    util::{parse_b58, DRK_ID, GOV_ID},
    Client, MoneyWallet,
};

pub struct JsonRpcInterface {
    client: Arc<Mutex<Client>>,
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
            Some("get_dao_addr") => return self.get_dao_addr(req.id, params).await,
            Some("get_votes") => return self.get_votes(req.id, params).await,
            Some("get_proposals") => return self.get_proposals(req.id, params).await,
            Some("dao_balance") => return self.dao_balance(req.id, params).await,
            Some("dao_bulla") => return self.dao_bulla(req.id, params).await,
            Some("user_balance") => return self.user_balance(req.id, params).await,
            Some("mint") => return self.mint_treasury(req.id, params).await,
            Some("keygen") => return self.keygen(req.id, params).await,
            Some("airdrop") => return self.airdrop_tokens(req.id, params).await,
            Some("propose") => return self.create_proposal(req.id, params).await,
            Some("vote") => return self.vote(req.id, params).await,
            Some("exec") => return self.execute(req.id, params).await,
            Some(_) | None => return JsonError::new(MethodNotFound, None, req.id).into(),
        }
    }
}

impl JsonRpcInterface {
    pub fn new(client: Client) -> Self {
        let client = Arc::new(Mutex::new(client));
        Self { client }
    }

    // --> {"method": "create", "params": []}
    // <-- {"result": "creating dao..."}
    async fn create_dao(&self, id: Value, params: &[Value]) -> JsonResult {
        let dao_proposer_limit = params[0].as_u64().unwrap();
        let dao_quorum = params[1].as_u64().unwrap();
        let dao_approval_ratio_quot = params[2].as_u64().unwrap();
        let dao_approval_ratio_base = params[3].as_u64().unwrap();

        let mut client = self.client.lock().await;

        let dao_bulla = client
            .create_dao(
                dao_proposer_limit,
                dao_quorum,
                dao_approval_ratio_quot,
                dao_approval_ratio_base,
                *GOV_ID,
            )
            .unwrap();

        let bulla: String = bs58::encode(dao_bulla.to_repr()).into_string();
        JsonResponse::new(json!(bulla), id).into()
    }

    // --> {"method": "get_dao_addr", "params": []}
    // <-- {"result": "getting dao public addr..."}
    async fn get_dao_addr(&self, id: Value, params: &[Value]) -> JsonResult {
        let mut client = self.client.lock().await;
        let pubkey = client.dao_wallet.get_public_key();
        let addr: String = bs58::encode(pubkey.to_bytes()).into_string();
        JsonResponse::new(json!(addr), id).into()
    }

    // --> {"method": "get_dao_addr", "params": []}
    // <-- {"result": "getting dao public addr..."}
    async fn get_votes(&self, id: Value, params: &[Value]) -> JsonResult {
        let mut client = self.client.lock().await;
        let vote_notes = client.dao_wallet.get_votes();
        let mut vote_data = vec![];

        for note in vote_notes {
            let vote_option = note.vote.vote_option;
            let vote_value = note.vote_value;
            vote_data.push((vote_option, vote_value));
        }

        JsonResponse::new(json!(vote_data), id).into()
    }

    // --> {"method": "get_dao_addr", "params": []}
    // <-- {"result": "getting dao public addr..."}
    async fn get_proposals(&self, id: Value, params: &[Value]) -> JsonResult {
        let mut client = self.client.lock().await;
        let proposals = client.dao_wallet.get_proposals();
        let mut proposal_data = vec![];

        for proposal in proposals {
            let dest = proposal.dest;
            let amount = proposal.amount;
            let token_id = proposal.token_id;
            let token_id: String = bs58::encode(token_id.to_repr()).into_string();
            proposal_data.push((dest, amount, token_id));
        }

        JsonResponse::new(json!(proposal_data), id).into()
    }

    async fn dao_balance(&self, id: Value, params: &[Value]) -> JsonResult {
        let mut client = self.client.lock().await;
        let balance = client.dao_wallet.balances().unwrap();
        JsonResponse::new(json!(balance), id).into()
    }

    async fn dao_bulla(&self, id: Value, params: &[Value]) -> JsonResult {
        let mut client = self.client.lock().await;
        let dao_bullas = client.dao_wallet.bullas.clone();
        let mut bulla_vec = Vec::new();

        for bulla in dao_bullas {
            let dao_bulla: String = bs58::encode(bulla.0.to_repr()).into_string();
            bulla_vec.push(dao_bulla);
        }

        JsonResponse::new(json!(bulla_vec), id).into()
    }

    async fn user_balance(&self, id: Value, params: &[Value]) -> JsonResult {
        let mut client = self.client.lock().await;
        let nym = params[0].as_str().unwrap();

        let pubkey = PublicKey::from_str(nym).unwrap();

        let wallet = client.money_wallets.get(&pubkey).unwrap();
        let balance = wallet.balances().unwrap();
        JsonResponse::new(json!(balance), id).into()
    }
    // --> {"method": "mint_treasury", "params": []}
    // <-- {"result": "minting treasury..."}
    async fn mint_treasury(&self, id: Value, params: &[Value]) -> JsonResult {
        let mut client = self.client.lock().await;

        let token_supply = params[0].as_u64().unwrap();
        let addr = params[1].as_str().unwrap();
        let dao_addr = PublicKey::from_str(addr).unwrap();

        client.mint_treasury(*DRK_ID, token_supply, dao_addr).unwrap();

        JsonResponse::new(json!("DAO treasury minted successfully."), id).into()
    }

    // Create a new wallet for governance tokens.
    async fn keygen(&self, id: Value, _params: &[Value]) -> JsonResult {
        let mut client = self.client.lock().await;
        // let nym = params[0].as_str().unwrap().to_string();

        let keypair = Keypair::random(&mut OsRng);
        let signature_secret = SecretKey::random(&mut OsRng);
        let own_coins: Vec<(OwnCoin, bool)> = Vec::new();
        let money_wallet = MoneyWallet { keypair, signature_secret, own_coins };
        money_wallet.track(&mut client.states);

        client.money_wallets.insert(keypair.public, money_wallet);

        // let wallet = client.money_wallets.get(&nym).unwrap();
        // let pubkey = wallet.get_public_key();

        let addr: String = bs58::encode(keypair.public.to_bytes()).into_string();
        JsonResponse::new(json!(addr), id).into()
    }

    // --> {"method": "airdrop_tokens", "params": []}
    // <-- {"result": "airdropping tokens..."}
    async fn airdrop_tokens(&self, id: Value, params: &[Value]) -> JsonResult {
        let mut client = self.client.lock().await;
        let zk_bins = &client.zk_bins;

        let addr = PublicKey::from_str(params[0].as_str().unwrap()).unwrap();
        let value = params[1].as_u64().unwrap();

        client.airdrop_user(value, *GOV_ID, addr).unwrap();

        JsonResponse::new(json!("Tokens airdropped successfully."), id).into()
    }
    // --> {"method": "create_proposal", "params": []}
    // <-- {"result": "creating proposal..."}
    async fn create_proposal(&self, id: Value, params: &[Value]) -> JsonResult {
        let mut client = self.client.lock().await;

        let sender = params[0].as_str().unwrap();
        let recipient = params[1].as_str().unwrap();
        let amount = params[2].as_u64().unwrap();

        let recv_addr = PublicKey::from_str(recipient).unwrap();
        let sndr_addr = PublicKey::from_str(sender).unwrap();

        let proposal_bulla = client.propose(recv_addr, *DRK_ID, amount, sndr_addr).unwrap();
        let bulla: String = bs58::encode(proposal_bulla.to_repr()).into_string();

        JsonResponse::new(json!(bulla), id).into()
    }
    // --> {"method": "vote", "params": []}
    // <-- {"result": "voting..."}
    async fn vote(&self, id: Value, params: &[Value]) -> JsonResult {
        let mut client = self.client.lock().await;

        let addr = params[0].as_str().unwrap();
        let vote_str = params[1].as_str().unwrap();

        let addr = PublicKey::from_str(addr).unwrap();

        // This would be cleaner as a match statement,
        // but we need to sort out error handling first.
        let mut vote_bool = true;

        if vote_str == "yes" {}
        if vote_str == "no" {
            vote_bool = false
        }

        client.cast_vote(addr, vote_bool).unwrap();
        JsonResponse::new(json!("Vote cast successfully."), id).into()
    }
    // --> {"method": "execute", "params": []}
    // <-- {"result": "executing..."}
    async fn execute(&self, id: Value, params: &[Value]) -> JsonResult {
        let mut client = self.client.lock().await;

        let bulla_str = params[0].as_str().unwrap();
        let bulla: pallas::Base = parse_b58(bulla_str).unwrap();

        client.exec_proposal(bulla);

        JsonResponse::new(json!("Proposal executed successfully."), id).into()
    }
}
