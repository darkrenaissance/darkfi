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

use darkfi::rpc::{
    jsonrpc::{JsonRequest, JsonResult},
    util::JsonValue,
};
use log::{debug, error};
use surf::http::mime;

use super::MiningProxy;

impl MiningProxy {
    pub async fn monero_get_block_count(&self, id: u16, params: JsonValue) -> JsonResult {
        debug!(target: "rpc::monero", "get_block_count()");

        let req_body = JsonRequest::new("get_block_count", vec![].into()).stringify().unwrap();

        let client = surf::Client::new();
        let mut response = client
            .get(&self.monerod_rpc)
            .header("Content-Type", "application/json")
            .body(req_body)
            .send()
            .await
            .unwrap();

        println!("{:?}", String::from_utf8_lossy(&response.body_bytes().await.unwrap()));

        todo!()
    }

    pub async fn monero_on_get_block_hash(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_get_block_template(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_submit_block(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_generateblocks(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_get_last_block_header(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_get_block_header_by_hash(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_get_block_header_by_height(
        &self,
        id: u16,
        params: JsonValue,
    ) -> JsonResult {
        todo!()
    }

    pub async fn monero_get_block_headers_range(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_get_block(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_get_connections(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_get_info(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_hard_fork_info(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_set_bans(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_get_bans(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_banned(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_flush_txpool(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_get_output_histogram(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_get_version(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_get_coinbase_tx_sum(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_get_fee_estimate(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_get_alternate_chains(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_relay_tx(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_sync_info(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_get_txpool_backlog(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_get_output_distribution(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_get_miner_data(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_prune_blockchain(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_calc_pow(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_flush_cache(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_add_aux_pow(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }
}
