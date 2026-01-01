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

use std::time::Instant;

use darkfi::{
    rpc::{jsonrpc::JsonRequest, util::JsonValue},
    Error, Result,
};
use darkfi_sdk::{
    crypto::pasta_prelude::{Field, PrimeField},
    pasta::pallas,
};
use darkfi_serial::deserialize;
use rand::rngs::OsRng;
use rlnd::database::Membership;

use crate::RlndCli;

impl RlndCli {
    /// Auxiliary function to ping configured rlnd daemon for liveness.
    pub async fn ping(&self) -> Result<()> {
        println!("Executing ping request to rlnd...");
        let latency = Instant::now();
        let rep = self.rlnd_daemon_request("ping", &JsonValue::Array(vec![])).await?;
        let latency = latency.elapsed();
        println!("Got reply: {rep:?}");
        println!("Latency: {latency:?}");
        Ok(())
    }

    /// Auxiliary function to execute a request towards the configured rlnd daemon JSON-RPC endpoint.
    pub async fn rlnd_daemon_request(&self, method: &str, params: &JsonValue) -> Result<JsonValue> {
        let req = JsonRequest::new(method, params.clone());
        let rep = self.rpc_client.request(req).await?;
        Ok(rep)
    }

    /// Queries rlnd to register a new membership for given stake.
    pub async fn register_membership(&self, stake: u64) -> Result<(String, Membership)> {
        // Generate a new random membership identity
        let id = pallas::Base::random(&mut OsRng);
        let id = bs58::encode(&id.to_repr()).into_string();

        // Generate request params
        let params = JsonValue::Array(vec![
            JsonValue::String(id.clone()),
            JsonValue::String(stake.to_string()),
        ]);

        // Execute request
        let rep = self.rlnd_daemon_request("add_membership", &params).await?;

        // Parse response
        let membership = parse_membership(rep.get::<String>().unwrap())?;

        Ok((id, membership))
    }

    /// Queries rlnd to retrieve all memberships.
    pub async fn get_all_memberships(&self) -> Result<Vec<(String, Membership)>> {
        // Execute request
        let rep = self.rlnd_daemon_request("get_memberships", &JsonValue::Array(vec![])).await?;

        // Parse response
        let params = rep.get::<Vec<JsonValue>>().unwrap();
        let mut ret = Vec::with_capacity(params.len() / 2);
        for (index, param) in params.iter().enumerate() {
            // Skip second half of pair
            if index % 2 == 1 {
                continue
            }

            let id = param.get::<String>().unwrap().clone();
            let membership = parse_membership(params[index + 1].get::<String>().unwrap())?;
            ret.push((id, membership));
        }

        Ok(ret)
    }

    /// Queries rlnd to slash a membership.
    pub async fn slash_membership(&self, id: &str) -> Result<Membership> {
        // Generate request params
        let params = JsonValue::Array(vec![JsonValue::String(id.to_string())]);

        // Execute request
        let rep = self.rlnd_daemon_request("slash_membership", &params).await?;

        // Parse response
        let membership = parse_membership(rep.get::<String>().unwrap())?;

        Ok(membership)
    }
}

/// Auxiliary function to parse a `Membership` from a `JsonValue::String`.
pub fn parse_membership(membership: &str) -> Result<Membership> {
    let Ok(decoded_bytes) = bs58::decode(membership).into_vec() else {
        return Err(Error::ParseFailed("Invalid Membership"))
    };

    Ok(deserialize(&decoded_bytes)?)
}
