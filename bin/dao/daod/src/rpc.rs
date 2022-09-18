use std::sync::Arc;

use async_std::sync::Mutex;
use async_trait::async_trait;
use log::debug;

use serde_json::{json, Value};

use darkfi::rpc::{
    jsonrpc::{ErrorCode::*, JsonError, JsonRequest, JsonResponse, JsonResult},
    server::RequestHandler,
};

use crate::Demo;

pub struct JsonRpcInterface {
    demo: Arc<Mutex<Demo>>,
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
    pub fn new(demo: Demo) -> Self {
        let demo = Arc::new(Mutex::new(demo));
        Self { demo }
    }

    // TODO: add 3 params: dao_proposer_limit, dao_quorum, dao_approval_ratio
    // --> {"method": "create", "params": []}
    // <-- {"result": "creating dao..."}
    async fn create_dao(&self, id: Value, _params: &[Value]) -> JsonResult {
        let mut demo = self.demo.lock().await;
        // TODO: pass in DaoParams from CLI
        //let dao_bulla = demo.client.create_dao();
        // TODO: return dao_bulla to command line
        JsonResponse::new(json!("dao created"), id).into()
    }
    // --> {"method": "mint_treasury", "params": []}
    // <-- {"result": "minting treasury..."}
    async fn mint_treasury(&self, id: Value, _params: &[Value]) -> JsonResult {
        let mut demo = self.demo.lock().await;
        let zk_bins = &demo.zk_bins;
        // TODO: pass DAO params + zk_bins into mint_treasury
        // let tx = demo.cashier.mint_treasury();
        // demo.client.validate(tx);
        // demo.client.wallet.balances();
        JsonResponse::new(json!("tokens minted"), id).into()
    }

    // Create a new wallet for governance tokens.
    // TODO: must pass a string identifier like alice, bob, charlie
    async fn keygen(&self, id: Value, _params: &[Value]) -> JsonResult {
        let mut demo = self.demo.lock().await;
        // TODO: pass string id
        //demo.client.new_money_wallet(alice);
        //let wallet = demo.client.money_wallets.get(alice) {
        //      Some(wallet) => wallet.keypair.public
        //}
        // TODO: return 'Alice: public key' to CLI
        JsonResponse::new(json!("created new keys"), id).into()
    }
    // --> {"method": "airdrop_tokens", "params": []}
    // <-- {"result": "airdropping tokens..."}
    // TODO: pass a string 'alice'
    async fn airdrop_tokens(&self, id: Value, _params: &[Value]) -> JsonResult {
        let mut demo = self.demo.lock().await;
        let zk_bins = &demo.zk_bins;
        //let keypair_public = demo.client.money_wallets.get(alice) {
        //      Some(wallet) => wallet.keypair.public
        //};
        //let transaction = demo.cashier.airdrop(keypair_public, zk_bins);
        // demo.client.validate(tx);
        //
        // demo.client.money_wallets.get(alice) {
        //      Some(wallet) => wallet.balances()
        // }
        // TODO: return wallet balance to command line
        JsonResponse::new(json!("tokens airdropped"), id).into()
    }
    // --> {"method": "create_proposal", "params": []}
    // <-- {"result": "creating proposal..."}
    // TODO: pass string 'alice' and dao bulla
    async fn create_proposal(&self, id: Value, _params: &[Value]) -> JsonResult {
        let mut demo = self.demo.lock().await;
        // let dao_params = self.demo.client.dao_wallet.params.get(bulla);
        //self.demo.client.dao_wallet.propose(dao_params).unwrap();
        // TODO: return proposal data and Proposal to CLI
        JsonResponse::new(json!("proposal created"), id).into()
    }
    // --> {"method": "vote", "params": []}
    // <-- {"result": "voting..."}
    // TODO: pass string 'alice', dao bulla, and Proposal
    // TODO: must pass yes or no, convert to bool
    async fn vote(&self, id: Value, _params: &[Value]) -> JsonResult {
        let mut demo = self.demo.lock().await;
        // let dao_params = self.demo.client.dao_wallet.params.get(bulla);
        // let dao_key = self.demo.client.dao_wallet.keypair.private;
        //
        // demo.client.money_wallets.get(alice) {
        //      Some(wallet) => {
        //      wallet.vote(dao_params)
        //      let tx = wallet.vote(dao_params, vote_option, proposal)
        //      demo.client.validate(tx);
        //      demo.client.dao_wallet.read_vote(tx);
        //      }
        // }
        //
        JsonResponse::new(json!("voted"), id).into()
    }
    // --> {"method": "execute", "params": []}
    // <-- {"result": "executing..."}
    async fn execute(&self, id: Value, _params: &[Value]) -> JsonResult {
        let mut demo = self.demo.lock().await;
        // demo.client.dao_wallet.build_exec_tx(proposal, proposal_bulla)
        //demo.exec().unwrap();
        JsonResponse::new(json!("executed"), id).into()
    }
}
