/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

use std::{str::FromStr, sync::Arc};

use async_std::sync::Mutex;
use async_trait::async_trait;
use darkfi_sdk::{
    crypto::{Keypair, PublicKey, SecretKey},
    pasta::group::ff::PrimeField,
};
use log::{debug, error};
use rand::rngs::OsRng;
use serde_json::{json, Value};

use darkfi::rpc::{
    jsonrpc::{ErrorCode::*, JsonError, JsonRequest, JsonResponse, JsonResult},
    server::RequestHandler,
};

use crate::{
    contract::money::state::OwnCoin,
    error::{server_error, RpcError},
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
        let dao_proposer_limit = params[0].as_u64();
        if dao_proposer_limit.is_none() {
            return JsonError::new(InvalidParams, None, id).into()
        }
        let dao_proposer_limit = dao_proposer_limit.unwrap();

        let dao_quorum = params[1].as_u64();
        if dao_quorum.is_none() {
            return JsonError::new(InvalidParams, None, id).into()
        }
        let dao_quorum = dao_quorum.unwrap();

        let dao_approval_ratio_quot = params[2].as_u64();
        if dao_approval_ratio_quot.is_none() {
            return JsonError::new(InvalidParams, None, id).into()
        }
        let dao_approval_ratio_quot = dao_approval_ratio_quot.unwrap();

        let dao_approval_ratio_base = params[3].as_u64();
        if dao_approval_ratio_base.is_none() {
            return JsonError::new(InvalidParams, None, id).into()
        }
        let dao_approval_ratio_base = dao_approval_ratio_base.unwrap();

        let mut client = self.client.lock().await;

        match client.create_dao(
            dao_proposer_limit,
            dao_quorum,
            dao_approval_ratio_quot,
            dao_approval_ratio_base,
            *GOV_ID,
        ) {
            Ok(bulla) => {
                let bulla: String = bs58::encode(bulla.to_repr()).into_string();
                JsonResponse::new(json!(bulla), id).into()
            }
            Err(e) => {
                error!("Failed to create DAO: {}", e);
                server_error(RpcError::Create, id)
            }
        }
    }

    // --> {"method": "get_dao_addr", "params": []}
    // <-- {"result": "getting dao public addr..."}
    async fn get_dao_addr(&self, id: Value, _params: &[Value]) -> JsonResult {
        let client = self.client.lock().await;
        let pubkey = client.dao_wallet.get_public_key();
        //let addr: String = bs58::encode(pubkey.to_bytes()).into_string();
        let addr: String = pubkey.to_string();
        JsonResponse::new(json!(addr), id).into()
    }

    // --> {"method": "get_dao_addr", "params": []}
    // <-- {"result": "getting dao public addr..."}
    async fn get_votes(&self, id: Value, _params: &[Value]) -> JsonResult {
        let client = self.client.lock().await;
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
    async fn get_proposals(&self, id: Value, _params: &[Value]) -> JsonResult {
        let client = self.client.lock().await;
        let proposals = client.dao_wallet.get_proposals();
        let mut proposal_data = vec![];

        for proposal in proposals {
            let dest = proposal.dest;
            let amount = proposal.amount;
            let token_id = format!("{}", proposal.token_id);
            //let dest: String = bs58::encode(dest.to_bytes()).into_string();
            let dest = dest.to_string();
            proposal_data.push((dest, amount, token_id));
        }

        JsonResponse::new(json!(proposal_data), id).into()
    }

    async fn dao_balance(&self, id: Value, _params: &[Value]) -> JsonResult {
        let client = self.client.lock().await;
        let balance = client.dao_wallet.balances().unwrap();
        JsonResponse::new(json!(balance), id).into()
    }

    async fn dao_bulla(&self, id: Value, _params: &[Value]) -> JsonResult {
        let client = self.client.lock().await;
        let dao_bullas = client.dao_wallet.bullas.clone();
        let mut bulla_vec = Vec::new();

        for bulla in dao_bullas {
            let dao_bulla: String = bs58::encode(bulla.0.to_repr()).into_string();
            bulla_vec.push(dao_bulla);
        }

        JsonResponse::new(json!(bulla_vec), id).into()
    }

    async fn user_balance(&self, id: Value, params: &[Value]) -> JsonResult {
        let client = self.client.lock().await;
        let nym = params[0].as_str();
        if nym.is_none() {
            return JsonError::new(InvalidParams, None, id).into()
        }
        let nym = nym.unwrap();

        match PublicKey::from_str(nym) {
            Ok(key) => match client.money_wallets.get(&key.to_bytes()) {
                Some(wallet) => {
                    let balance = wallet.balances().unwrap();
                    JsonResponse::new(json!(balance), id).into()
                }
                None => {
                    error!("No wallet found for provided key");
                    server_error(RpcError::Balance, id)
                }
            },
            Err(_) => {
                error!("Could not parse PublicKey from string");
                server_error(RpcError::Parse, id)
            }
        }
    }

    // --> {"method": "mint_treasury", "params": []}
    // <-- {"result": "minting treasury..."}
    async fn mint_treasury(&self, id: Value, params: &[Value]) -> JsonResult {
        let mut client = self.client.lock().await;

        let token_supply = params[0].as_u64();
        if token_supply.is_none() {
            return JsonError::new(InvalidParams, None, id).into()
        }
        let token_supply = token_supply.unwrap();

        let addr = params[1].as_str();
        if addr.is_none() {
            return JsonError::new(InvalidParams, None, id).into()
        }
        let addr = addr.unwrap();

        match PublicKey::from_str(addr) {
            Ok(dao_addr) => match client.mint_treasury(*DRK_ID, token_supply, dao_addr) {
                Ok(_) => JsonResponse::new(json!("DAO treasury minted successfully."), id).into(),
                Err(e) => {
                    error!("Failed to mint treasury: {}", e);
                    server_error(RpcError::Mint, id)
                }
            },
            Err(_) => {
                error!("Failed to parse PublicKey from String");
                server_error(RpcError::Parse, id)
            }
        }
    }

    // Create a new wallet for governance tokens.
    async fn keygen(&self, id: Value, _params: &[Value]) -> JsonResult {
        let mut client = self.client.lock().await;
        // let nym = params[0].as_str().unwrap().to_string();

        let keypair = Keypair::random(&mut OsRng);
        let signature_secret = SecretKey::random(&mut OsRng);
        let own_coins: Vec<(OwnCoin, bool)> = Vec::new();
        let money_wallet = MoneyWallet { keypair, signature_secret, own_coins };

        match money_wallet.track(&mut client.states) {
            Ok(_) => {
                client.money_wallets.insert(keypair.public.to_bytes(), money_wallet);
                //let addr: String = bs58::encode(keypair.public.to_bytes()).into_string();
                let addr: String = keypair.public.to_string();
                JsonResponse::new(json!(addr), id).into()
            }
            Err(e) => {
                error!("Failed to airdrop tokens: {}", e);
                server_error(RpcError::Keygen, id)
            }
        }
    }

    // --> {"method": "airdrop_tokens", "params": []}
    // <-- {"result": "airdropping tokens..."}
    async fn airdrop_tokens(&self, id: Value, params: &[Value]) -> JsonResult {
        let mut client = self.client.lock().await;
        // let zk_bins = &client.zk_bins;

        let addr = params[0].as_str();
        if addr.is_none() {
            return JsonError::new(InvalidParams, None, id).into()
        }
        let addr = addr.unwrap();

        let value = params[1].as_u64();
        if value.is_none() {
            return JsonError::new(InvalidParams, None, id).into()
        }
        let value = value.unwrap();

        match PublicKey::from_str(addr) {
            Ok(key) => match client.airdrop_user(value, *GOV_ID, key) {
                Ok(_) => JsonResponse::new(json!("Tokens airdropped successfully."), id).into(),
                Err(e) => {
                    error!("Failed to airdrop tokens: {}", e);
                    server_error(RpcError::Airdrop, id)
                }
            },
            Err(_) => {
                error!("Failed parsing PublicKey from String");
                server_error(RpcError::Parse, id)
            }
        }
    }
    // --> {"method": "create_proposal", "params": []}
    // <-- {"result": "creating proposal..."}
    async fn create_proposal(&self, id: Value, params: &[Value]) -> JsonResult {
        let mut client = self.client.lock().await;

        if params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let sender = params[0].as_str();
        if sender.is_none() {
            return JsonError::new(InvalidParams, None, id).into()
        }
        let sender = sender.unwrap();

        let recipient = params[1].as_str();
        if recipient.is_none() {
            return JsonError::new(InvalidParams, None, id).into()
        }
        let recipient = recipient.unwrap();

        let amount = params[2].as_u64();
        if amount.is_none() {
            return JsonError::new(InvalidParams, None, id).into()
        }
        let amount = amount.unwrap();

        let recv_addr = PublicKey::from_str(recipient);
        if recv_addr.is_err() {
            return JsonError::new(InvalidParams, None, id).into()
        }
        let recv_addr = recv_addr.unwrap();

        let sndr_addr = PublicKey::from_str(sender);
        if sndr_addr.is_err() {
            return JsonError::new(InvalidParams, None, id).into()
        }
        let sndr_addr = sndr_addr.unwrap();

        match client.propose(recv_addr, *DRK_ID, amount, sndr_addr) {
            Ok(bulla) => {
                let bulla: String = bs58::encode(bulla.to_repr()).into_string();

                JsonResponse::new(json!(bulla), id).into()
            }
            Err(e) => {
                error!("Failed to make Proposal: {}", e);
                server_error(RpcError::Propose, id)
            }
        }
    }
    // --> {"method": "vote", "params": []}
    // <-- {"result": "voting..."}
    async fn vote(&self, id: Value, params: &[Value]) -> JsonResult {
        let mut client = self.client.lock().await;
        let mut vote_bool = true;

        let addr = params[0].as_str();
        if addr.is_none() {
            return JsonError::new(InvalidParams, None, id).into()
        }
        let addr = addr.unwrap();

        let balance = match PublicKey::from_str(addr) {
            Ok(key) => match client.money_wallets.get(&key.to_bytes()) {
                Some(wallet) => {
                    let balance = wallet.balances().unwrap();
                    let token_id = format!("{}", *GOV_ID);
                    if balance.get(&token_id).is_some() {
                        *balance.get(&token_id).unwrap()
                    } else {
                        error!("Balance is empty");
                        0
                    }
                }
                None => {
                    error!("No wallet found for provided key");
                    0
                }
            },
            Err(_) => {
                error!("Could not parse PublicKey from string");
                0
            }
        };

        let vote_str = params[1].as_str();
        if vote_str.is_none() {
            return JsonError::new(InvalidParams, None, id).into()
        }
        let vote_str = vote_str.unwrap();

        match vote_str {
            "yes" => {}
            "no" => vote_bool = false,
            _ => return JsonError::new(InvalidParams, None, id).into(),
        }

        match PublicKey::from_str(addr) {
            Ok(key) => match client.cast_vote(key, vote_bool) {
                Ok(_) => {
                    let rep = format!(
                        "Vote cast successfully, you voted {} with value {} GOV.",
                        vote_str, balance
                    );
                    JsonResponse::new(json!(rep), id).into()
                }
                Err(e) => {
                    error!("Failed casting vote: {}", e);
                    server_error(RpcError::Vote, id)
                }
            },
            Err(_) => {
                error!("Failed parsing PublicKey from String");
                server_error(RpcError::Parse, id)
            }
        }
    }
    // --> {"method": "execute", "params": []}
    // <-- {"result": "executing..."}
    async fn execute(&self, id: Value, params: &[Value]) -> JsonResult {
        let mut client = self.client.lock().await;

        let bulla_str = params[0].as_str();
        if bulla_str.is_none() {
            return JsonError::new(InvalidParams, None, id).into()
        }
        let bulla_str = bulla_str.unwrap();

        let bulla = parse_b58(bulla_str);
        match bulla {
            Ok(bulla) => match client.exec_proposal(bulla) {
                Ok(_) => JsonResponse::new(json!("Proposal executed successfully."), id).into(),
                Err(e) => {
                    // Reject proposal instead of returning error?
                    error!("Failed executing proposal: {}", e);
                    server_error(RpcError::Exec, id)
                }
            },
            Err(e) => {
                error!("Failed parsing bulla: {}", e);
                server_error(RpcError::Parse, id)
            }
        }
    }
}
