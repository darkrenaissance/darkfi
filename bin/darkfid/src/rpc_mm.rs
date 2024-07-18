/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use std::{
    collections::HashMap,
    io::{self, ErrorKind},
};

use log::error;
use tinyjson::JsonValue;

use crate::Darkfid;

impl Darkfid {
    pub async fn handle_mm_rpc_request(&self, mut req: tide::Request<()>) -> tide::Result {
        let body = req.body_string().await?;

        let parsed: JsonValue = match body.parse() {
            Ok(v) => v,
            Err(e) => {
                error!("Failed parsing JSON on MM request: {}", e);
                return Err(tide::Error::new(
                    400,
                    io::Error::new(ErrorKind::Unsupported, "Bad request"),
                ))
            }
        };

        println!("{:#?}", parsed);

        let json_object: &HashMap<String, _> = match parsed.get() {
            Some(v) => v,
            None => {
                return Err(tide::Error::new(
                    400,
                    io::Error::new(ErrorKind::Unsupported, "Bad request"),
                ))
            }
        };

        // FIXME: TODO: Handle missing JSON keys, note P2Pool doesn't always send "params"

        let method: &String = json_object["method"].get().unwrap();
        match method.as_str() {
            "merge_mining_get_chain_id" => self.merge_mining_get_chain_id().await,
            "merge_mining_get_aux_block" => todo!(),
            "merge_mining_submit_solution" => todo!(),
            _ => Err(tide::Error::new(
                400,
                io::Error::new(ErrorKind::Unsupported, "Unsupported method"),
            )),
        }
    }

    async fn merge_mining_get_chain_id(&self) -> tide::Result {
        let (_, chain_id) = self.validator.blockchain.genesis()?;
        Ok(chain_id.as_string().into())
    }
}
