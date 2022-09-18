use std::sync::Arc;

use async_std::sync::Mutex;
use async_trait::async_trait;
use log::debug;
use pasta_curves::group::ff::PrimeField;

use serde_json::{json, Value};

use darkfi::rpc::{
    jsonrpc::{ErrorCode::*, JsonError, JsonRequest, JsonResponse, JsonResult},
    server::RequestHandler,
};

use crate::{util::GDRK_ID, Client};

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
        // TODO: return dao_bulla to command line
        // Encode as base58.

        let bulla: String = bs58::encode(dao_bulla.to_repr()).into_string();
        JsonResponse::new(json!(bulla), id).into()
    }
    // --> {"method": "mint_treasury", "params": []}
    // <-- {"result": "minting treasury..."}
    async fn mint_treasury(&self, id: Value, _params: &[Value]) -> JsonResult {
        let mut client = self.client.lock().await;
        let zk_bins = &client.zk_bins;
        // TODO: pass DAO params + zk_bins into mint_treasury
        //let tx = client.cashier.mint_treasury();
        // client.client.validate(tx);
        // client.client.wallet.balances();
        JsonResponse::new(json!("tokens minted"), id).into()
    }

    // Create a new wallet for governance tokens.
    // TODO: must pass a string identifier like alice, bob, charlie
    async fn keygen(&self, id: Value, _params: &[Value]) -> JsonResult {
        let mut client = self.client.lock().await;
        // TODO: pass string id
        //client.client.new_money_wallet(alice);
        //let wallet = client.client.money_wallets.get(alice) {
        //      Some(wallet) => wallet.keypair.public
        //}
        // TODO: return 'Alice: public key' to CLI
        JsonResponse::new(json!("created new keys"), id).into()
    }
    // --> {"method": "airdrop_tokens", "params": []}
    // <-- {"result": "airdropping tokens..."}
    // TODO: pass a string 'alice'
    async fn airdrop_tokens(&self, id: Value, _params: &[Value]) -> JsonResult {
        let mut client = self.client.lock().await;
        let zk_bins = &client.zk_bins;
        //let keypair_public = client.client.money_wallets.get(alice) {
        //      Some(wallet) => wallet.keypair.public
        //};
        //let transaction = client.cashier.airdrop(keypair_public, zk_bins);
        // client.client.validate(tx);
        //
        // client.client.money_wallets.get(alice) {
        //      Some(wallet) => wallet.balances()
        // }
        // TODO: return wallet balance to command line
        JsonResponse::new(json!("tokens airdropped"), id).into()
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
