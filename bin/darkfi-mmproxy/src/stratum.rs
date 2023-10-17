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
    jsonrpc::{ErrorCode, JsonError, JsonResult},
    util::JsonValue,
};

use super::MiningProxy;

impl MiningProxy {
    pub async fn stratum_login(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 1 || !params[0].is_object() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        todo!()
    }

    pub async fn stratum_job(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn stratum_submit(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn stratum_keepalived(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }
}
