/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

use darkfi::{net::transport::Dialer, Result};
use darkfi_serial::{
    async_trait, serialize_async, AsyncDecodable, AsyncEncodable, SerialDecodable, SerialEncodable,
};
use std::time::UNIX_EPOCH;
use url::Url;

use evgrd::{VersionMessage, MSG_SENDEVENT};

#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct Privmsg {
    pub channel: String,
    pub nick: String,
    pub msg: String,
}

async fn amain() -> Result<()> {
    let endpoint = "tcp://127.0.0.1:5588";
    let endpoint = Url::parse(endpoint)?;

    let dialer = Dialer::new(endpoint, None).await?;
    let timeout = std::time::Duration::from_secs(60);

    println!("Connecting...");
    let mut stream = dialer.dial(Some(timeout)).await?;
    println!("Connected!");

    let version = VersionMessage::new();
    version.encode_async(&mut stream).await?;

    let server_version = VersionMessage::decode_async(&mut stream).await?;
    println!("Server version: {}", server_version.protocol_version);

    let msg = Privmsg {
        channel: "#random".to_string(),
        nick: "anon".to_string(),
        msg: "i'm so random!".to_string(),
    };
    let timestamp = UNIX_EPOCH.elapsed().unwrap().as_millis() as u64;

    MSG_SENDEVENT.encode_async(&mut stream).await?;
    timestamp.encode_async(&mut stream).await?;

    let content: Vec<u8> = serialize_async(&msg).await;
    content.encode_async(&mut stream).await?;

    Ok(())
}

fn main() {
    let is_success = smol::block_on(amain());
    println!("Finished: {is_success:?}");
}
