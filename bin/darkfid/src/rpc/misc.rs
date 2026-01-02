/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use tinyjson::JsonValue;

use darkfi::{
    rpc::jsonrpc::{JsonResponse, JsonResult},
    util::time::Timestamp,
};

use crate::DarkfiNode;

impl DarkfiNode {
    // RPCAPI:
    // Returns current system clock as a UNIX timestamp.
    //
    // --> {"jsonrpc": "2.0", "method": "clock", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": 1767015913, "id": 1}
    pub async fn clock(&self, id: u16, _params: JsonValue) -> JsonResult {
        JsonResponse::new((Timestamp::current_time().inner() as f64).into(), id).into()
    }
}
