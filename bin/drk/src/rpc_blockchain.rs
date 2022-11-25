/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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

use anyhow::{anyhow, Result};
use darkfi::{
    rpc::jsonrpc::{JsonRequest, JsonResult},
    system::Subscriber,
};
use serde_json::json;

use super::Drk;

impl Drk {
    /// Subscribes to darkfid's JSON-RPC notification endpoint that serves
    /// new finalized blocks. Upon receiving them, all the transactions are
    /// scanned and we check if any of them call the money contract, and if
    /// the payments are intended for us. If so, we decrypt them and append
    /// the metadata to our wallet.
    pub async fn subscribe_blocks(&self) -> Result<()> {
        eprintln!("Subscribing to receive notifications of incoming blocks");
        let subscriber = Subscriber::new();
        let subscription = subscriber.clone().subscribe().await;

        let req = JsonRequest::new("blockchain.subscribe_blocks", json!([]));
        self.rpc_client.subscribe(req, subscriber).await?;

        let e = loop {
            match subscription.receive().await {
                JsonResult::Notification(n) => {
                    println!("Got Block notification: {:?}", n);
                }

                JsonResult::Error(e) => {
                    // Some error happened in the transmission
                    break anyhow!("Got error from JSON-RPC: {:?}", e)
                }

                x => {
                    // And this is weird
                    break anyhow!("Got unexpected data from JSON-RPC: {:?}", x)
                }
            }
        };

        Err(e)
    }
}
