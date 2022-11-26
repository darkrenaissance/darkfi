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

use darkfi::{
    rpc::{
        client::RpcClient,
        jsonrpc::{JsonRequest, JsonResult},
    },
    system::{Subscriber, SubscriberPtr},
    Result,
};
use futures::join;
use serde_json::json;
use url::Url;

async fn listen(subscriber: SubscriberPtr<JsonResult>) -> Result<()> {
    let subscription = subscriber.subscribe().await;
    loop {
        // Listen subscription for notifications
        let notification = subscription.receive().await;
        match notification {
            JsonResult::Notification(n) => {
                println!("Got notification: {:?}", n);
            }
            JsonResult::Error(e) => {
                println!("Client returned an error: {}", serde_json::to_string(&e)?);
                break
            }
            _ => {
                println!("Client returned an unexpected reply.");
                break
            }
        }
    }
    subscription.unsubscribe().await;

    Ok(())
}

#[async_std::main]
async fn main() -> Result<()> {
    let endpoint = Url::parse("tcp://127.0.0.1:18927")?;
    let notif_channel = "blockchain.notify_blocks";
    println!("Creating subscriber for channel: {}", notif_channel);
    let subscriber: SubscriberPtr<JsonResult> = Subscriber::new();

    println!("Creating client for endpoint: {}", endpoint);
    let rpc_client = RpcClient::new(endpoint).await?;
    println!("Subscribing client");
    let req = JsonRequest::new("blockchain.notify_blocks", json!([]));

    println!("Starting listening");
    let result = join!(listen(subscriber.clone()), rpc_client.subscribe(req, subscriber));
    match result.0 {
        Ok(_) => {}
        Err(e) => println!("Listener failed: {}", e),
    }
    match result.1 {
        Ok(_) => {}
        Err(e) => println!("Subscriber failed: {}", e),
    }

    Ok(())
}
