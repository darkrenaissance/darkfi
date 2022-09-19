use std::sync::Arc;

use async_std::sync::Mutex;
use async_trait::async_trait;
use log::debug;
use pasta_curves::{group::ff::PrimeField, pallas};
use std::str::FromStr;

use serde_json::{json, Value};

use darkfi::{
    crypto::keypair::PublicKey,
    rpc::{
        jsonrpc::{ErrorCode::*, JsonError, JsonRequest, JsonResponse, JsonResult},
        server::RequestHandler,
    },
};

use crate::{
    util::{parse_b58, GDRK_ID, XDRK_ID},
    Client,
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
        // TODO: error handling
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
                *GDRK_ID,
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

    // --> {"method": "mint_treasury", "params": []}
    // <-- {"result": "minting treasury..."}
    async fn mint_treasury(&self, id: Value, params: &[Value]) -> JsonResult {
        // TODO: error handling
        let mut client = self.client.lock().await;

        let token_supply = params[0].as_u64().unwrap();
        let addr = params[1].as_str().unwrap();
        let bulla = params[2].as_str().unwrap();

        let dao_bulla = parse_b58(bulla).unwrap();
        let dao_addr = PublicKey::from_str(addr).unwrap();

        //match PublicKey::from_str(addr) {
        //    Ok(addr) => {
        //        debug!(target: "daod::rpc", "Decoded correctly: {:?}", addr)
        //    }
        //    Err(e) => {
        //        debug!(target: "daod::rpc", "Decoded incorrectly: {}", e)
        //    }
        //}

        let balance = client.mint_treasury(*XDRK_ID, token_supply, dao_bulla, dao_addr).unwrap();

        JsonResponse::new(json!(balance), id).into()
    }

    // Create a new wallet for governance tokens.
    async fn keygen(&self, id: Value, params: &[Value]) -> JsonResult {
        debug!(target: "dao-demo::rpc::keygen()", "Received keygen request");
        let mut client = self.client.lock().await;
        let nym = params[0].as_str().unwrap().to_string();

        client.new_money_wallet(nym.clone());

        let wallet = client.money_wallets.get(&nym).unwrap();
        let pubkey = wallet.get_public_key();

        let addr: String = bs58::encode(pubkey.to_bytes()).into_string();
        JsonResponse::new(json!(addr), id).into()
    }
    // --> {"method": "airdrop_tokens", "params": []}
    // <-- {"result": "airdropping tokens..."}
    async fn airdrop_tokens(&self, id: Value, params: &[Value]) -> JsonResult {
        let mut client = self.client.lock().await;
        let zk_bins = &client.zk_bins;

        let nym = params[0].as_str().unwrap().to_string();
        let value = params[1].as_u64().unwrap();

        client.airdrop_user(value, *GDRK_ID, nym.clone()).unwrap();
        let balance = client.query_balance(nym.clone()).unwrap();

        JsonResponse::new(json!(balance), id).into()
    }
    // --> {"method": "create_proposal", "params": []}
    // <-- {"result": "creating proposal..."}
    // TODO: pass string 'alice' and dao bulla
    async fn create_proposal(&self, id: Value, _params: &[Value]) -> JsonResult {
        let mut client = self.client.lock().await;
        // let dao_params = self.client.client.dao_wallet.params.get(bulla);
        //self.client.client.dao_wallet.propose(dao_params).unwrap();
        // TODO: return proposal data and Proposal to CLI
        JsonResponse::new(json!("proposal created"), id).into()
    }
    // --> {"method": "vote", "params": []}
    // <-- {"result": "voting..."}
    // TODO: pass string 'alice', dao bulla, and Proposal
    // TODO: must pass yes or no, convert to bool
    async fn vote(&self, id: Value, _params: &[Value]) -> JsonResult {
        let mut client = self.client.lock().await;
        // let dao_params = self.client.client.dao_wallet.params.get(bulla);
        // let dao_key = self.client.client.dao_wallet.keypair.private;
        //
        // client.client.money_wallets.get(alice) {
        //      Some(wallet) => {
        //      wallet.vote(dao_params)
        //      let tx = wallet.vote(dao_params, vote_option, proposal)
        //      client.client.validate(tx);
        //      client.client.dao_wallet.read_vote(tx);
        //      }
        // }
        //
        JsonResponse::new(json!("voted"), id).into()
    }
    // --> {"method": "execute", "params": []}
    // <-- {"result": "executing..."}
    async fn execute(&self, id: Value, _params: &[Value]) -> JsonResult {
        let mut client = self.client.lock().await;
        // client.client.dao_wallet.build_exec_tx(proposal, proposal_bulla)
        //client.exec().unwrap();
        JsonResponse::new(json!("executed"), id).into()
    }
}
