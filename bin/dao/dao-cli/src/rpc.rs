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

use serde_json::{json, Value};

use darkfi::{rpc::jsonrpc::JsonRequest, Result};

use crate::Rpc;

impl Rpc {
    // --> {"jsonrpc": "2.0", "method": "create", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "creating dao...", "id": 42}
    pub async fn create(
        &self,
        dao_proposer_limit: u64,
        dao_quorum: u64,
        dao_approval_ratio_quot: u64,
        dao_approval_ratio_base: u64,
    ) -> Result<Value> {
        let req = JsonRequest::new(
            "create",
            json!([
                dao_proposer_limit,
                dao_quorum,
                dao_approval_ratio_quot,
                dao_approval_ratio_base,
            ]),
        );
        self.client.request(req).await
    }

    // --> {"jsonrpc": "2.0", "method": "mint", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "minting tokens...", "id": 42}
    pub async fn addr(&self) -> Result<Value> {
        let req = JsonRequest::new("get_dao_addr", json!([]));
        self.client.request(req).await
    }

    // --> {"jsonrpc": "2.0", "method": "mint", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "minting tokens...", "id": 42}
    pub async fn mint(&self, token_supply: u64, dao_addr: String) -> Result<Value> {
        let req = JsonRequest::new("mint", json!([token_supply, dao_addr]));
        self.client.request(req).await
    }

    // --> {"jsonrpc": "2.0", "method": "airdrop", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "airdropping tokens...", "id": 42}
    pub async fn airdrop(&self, nym: String, value: u64) -> Result<Value> {
        let req = JsonRequest::new("airdrop", json!([nym, value]));
        self.client.request(req).await
    }

    // --> {"jsonrpc": "2.0", "method": "airdrop", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "airdropping tokens...", "id": 42}
    pub async fn keygen(&self) -> Result<Value> {
        let req = JsonRequest::new("keygen", json!([]));
        self.client.request(req).await
    }

    // --> {"jsonrpc": "2.0", "method": "airdrop", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "airdropping tokens...", "id": 42}
    pub async fn dao_balance(&self) -> Result<Value> {
        let req = JsonRequest::new("dao_balance", json!([]));
        self.client.request(req).await
    }

    // --> {"jsonrpc": "2.0", "method": "airdrop", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "airdropping tokens...", "id": 42}
    pub async fn dao_bulla(&self) -> Result<Value> {
        let req = JsonRequest::new("dao_bulla", json!([]));
        self.client.request(req).await
    }

    // --> {"jsonrpc": "2.0", "method": "airdrop", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "airdropping tokens...", "id": 42}
    pub async fn user_balance(&self, nym: String) -> Result<Value> {
        let req = JsonRequest::new("user_balance", json!([nym]));
        self.client.request(req).await
    }

    // --> {"jsonrpc": "2.0", "method": "propose", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "creating proposal...", "id": 42}
    pub async fn propose(&self, sender: String, recipient: String, amount: u64) -> Result<Value> {
        let req = JsonRequest::new("propose", json!([sender, recipient, amount]));
        self.client.request(req).await
    }

    // --> {"jsonrpc": "2.0", "method": "vote", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "voting...", "id": 42}
    pub async fn vote(&self, nym: String, vote: String) -> Result<Value> {
        let req = JsonRequest::new("vote", json!([nym, vote]));
        self.client.request(req).await
    }

    // --> {"jsonrpc": "2.0", "method": "exec", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "executing...", "id": 42}
    pub async fn get_votes(&self) -> Result<Value> {
        let req = JsonRequest::new("get_votes", json!([]));
        self.client.request(req).await
    }

    // --> {"jsonrpc": "2.0", "method": "exec", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "executing...", "id": 42}
    pub async fn get_proposals(&self) -> Result<Value> {
        let req = JsonRequest::new("get_proposals", json!([]));
        self.client.request(req).await
    }

    // --> {"jsonrpc": "2.0", "method": "exec", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "executing...", "id": 42}
    pub async fn exec(&self, bulla: String) -> Result<Value> {
        let req = JsonRequest::new("exec", json!([bulla]));
        self.client.request(req).await
    }
}
