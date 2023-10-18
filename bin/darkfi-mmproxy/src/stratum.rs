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

use std::collections::HashMap;

use darkfi::rpc::{
    jsonrpc::{ErrorCode, JsonError, JsonResponse, JsonResult},
    util::JsonValue,
};
use uuid::Uuid;

use super::MiningProxy;

/// Algo string representing Monero's RandomX
pub const RANDOMX_ALGO: &str = "rx/0";

impl MiningProxy {
    /// Stratum login method. `darkfi-mmproxy` will check that it is a valid worker
    /// login, and will also search for `RANDOMX_ALGO`.
    /// TODO: More proper error codes
    pub async fn stratum_login(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 1 || !params[0].is_object() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        let params = params[0].get::<HashMap<String, JsonValue>>().unwrap();

        if !params.contains_key("login") ||
            !params.contains_key("pass") ||
            !params.contains_key("agent") ||
            !params.contains_key("algo")
        {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        let Some(login) = params["login"].get::<String>() else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };

        let Some(pass) = params["pass"].get::<String>() else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };

        let Some(agent) = params["agent"].get::<String>() else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };

        let Some(algos) = params["algo"].get::<Vec<JsonValue>>() else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };

        // We'll only support rx/0 algo.
        let mut found_xmr_algo = false;
        for algo in algos {
            if !algo.is_string() {
                return JsonError::new(ErrorCode::InvalidParams, None, id).into()
            }

            if algo.get::<String>().unwrap() == RANDOMX_ALGO {
                found_xmr_algo = true;
                break
            }
        }

        if !found_xmr_algo {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        // Check valid login
        let Some(known_pass) = self.logins.get(login) else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };

        if known_pass != pass {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        // Login success, generate UUID
        let uuid = Uuid::new_v4();

        todo!()
    }

    pub async fn stratum_job(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn stratum_submit(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    /// Non standard but widely supported protocol extension. Miner sends `keepalived`
    /// to prevent connection timeout.
    pub async fn stratum_keepalived(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 1 || !params[0].is_object() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        let params = params[0].get::<HashMap<String, JsonValue>>().unwrap();

        if !params.contains_key("id") {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        let Some(uuid) = params["id"].get::<String>() else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };

        if self.workers.read().await.contains_key(uuid) {
            return JsonResponse::new(
                JsonValue::Object(HashMap::from([(
                    "status".to_string(),
                    JsonValue::String("KEEPALIVED".to_string()),
                )])),
                id,
            )
            .into()
        }

        return JsonError::new(ErrorCode::InvalidParams, None, id).into()
    }
}
