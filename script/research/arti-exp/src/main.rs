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

use anyhow::Result;
use arti_client::{BootstrapBehavior, TorClient};
use async_std::io::{ReadExt, WriteExt};

#[async_std::main]
async fn main() -> Result<()> {
    //const ADDR: &str = "p7qaewjgnvnaeihhyybmoofd5avh665kr3awoxlh5rt6ox743kjdr6qd.onion";
    const ADDR: &str = "parazyd.org";

    let mut log_cfg = simplelog::ConfigBuilder::new();
    simplelog::TermLogger::init(
        simplelog::LevelFilter::Debug,
        log_cfg.build(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )?;

    let tor_client = TorClient::builder()
        .bootstrap_behavior(BootstrapBehavior::OnDemand)
        .create_unbootstrapped()?;

    let mut stream = tor_client.connect((ADDR, 80)).await?;

    let req = format!("GET / HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n", ADDR);

    stream.write_all(req.as_bytes()).await?;
    stream.flush().await?;

    let mut buf = vec![];
    stream.read_to_end(&mut buf).await?;

    println!("{}", String::from_utf8_lossy(&buf));

    Ok(())
}
