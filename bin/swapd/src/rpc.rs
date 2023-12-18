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

use std::collections::HashSet;

use darkfi::{
    rpc::{
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResponse, JsonResult},
        server::RequestHandler,
        util::JsonValue,
    },
    system::StoppableTaskPtr,
};
use darkfi_serial::async_trait;
use smol::lock::MutexGuard;

use crate::swapd::Swapd;

#[async_trait]
impl RequestHandler for Swapd {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        match req.method.as_str() {
            "ping" => self.pong(req.id, req.params).await,
            "hello" => self.hello(req.id, req.params).await,
            _ => JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
        }
    }

    async fn connections_mut(&self) -> MutexGuard<'_, HashSet<StoppableTaskPtr>> {
        self.rpc_connections.lock().await
    }
}

impl Swapd {
    // RPCAPI:
    // Use this kind of comment in order to have the RPC spec automatically
    // generated in the mdbook. You should be able to write any kind of
    // markdown in here.
    //
    // At the bottom, you should have the reqrep in JSON:
    //
    // --> {"jsonrpc": "2.0", "method": "hello", "params": ["hello"], "id": 42}
    // --> {"jsonrpc": "2.0", "result": "hello", "id": 42}
    async fn hello(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        JsonResponse::new(params[0].clone(), id).into()
    }
}
