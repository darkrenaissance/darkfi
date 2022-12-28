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

use serde_json::Value;

use darkfi::rpc::jsonrpc::{ErrorCode::ServerError, JsonError, JsonResult};

pub enum RpcError {
    AmountExceedsLimit = -32107,
    TimeLimitReached = -32108,
    ParseError = -32109,
    InternalError = -32110,
}

fn to_tuple(e: RpcError) -> (i64, String) {
    let msg = match e {
        RpcError::AmountExceedsLimit => "Amount requested is higher than the faucet limit",
        RpcError::TimeLimitReached => "Timeout not expired, try again later",
        RpcError::ParseError => "Parse error",
        RpcError::InternalError => "Internal error",
    };

    (e as i64, msg.to_string())
}

pub fn server_error(e: RpcError, id: Value) -> JsonResult {
    let (code, msg) = to_tuple(e);
    JsonError::new(ServerError(code), Some(msg), id).into()
}
