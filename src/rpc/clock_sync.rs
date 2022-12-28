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

//! Clock sync module
use std::{net::UdpSocket, time::Duration};

use log::debug;
use rand::prelude::SliceRandom;
use serde_json::json;
use url::Url;

use super::{client::RpcClient, jsonrpc::JsonRequest};
use crate::{util::time::Timestamp, Error, Result};

/// Clock sync parameters
const RETRIES: u8 = 10;
/// TODO: Loop through set of ntps, get their average response concurrenyly.
const NTP_ADDRESS: &str = "pool.ntp.org:123";
const EPOCH: i64 = 2208988800; // 1900

/// JSON-RPC request to a network peer (randomly selected), to
/// retrieve their current system clock.
async fn peer_request(peers: &[Url]) -> Result<Option<Timestamp>> {
    // Select peer, None if vector is empty.
    let peer = peers.choose(&mut rand::thread_rng());
    match peer {
        None => Ok(None),
        Some(p) => {
            // Create RPC client
            let rpc_client = RpcClient::new(p.clone()).await?;

            // Execute request
            let req = JsonRequest::new("clock", json!([]));
            let rep = rpc_client.oneshot_request(req).await?;

            // Parse response
            let timestamp: Timestamp = serde_json::from_value(rep)?;

            Ok(Some(timestamp))
        }
    }
}

/// Raw NTP request execution
pub async fn ntp_request() -> Result<Timestamp> {
    // Create socket
    let sock = UdpSocket::bind("0.0.0.0:0")?;
    sock.set_read_timeout(Some(Duration::from_secs(5)))?;
    sock.set_write_timeout(Some(Duration::from_secs(5)))?;

    // Execute request
    let mut packet = [0u8; 48];
    packet[0] = (3 << 6) | (4 << 3) | 3;
    sock.send_to(&packet, NTP_ADDRESS)?;

    // Parse response
    sock.recv(&mut packet[..])?;
    let (bytes, _) = packet[40..44].split_at(core::mem::size_of::<u32>());
    let num = u32::from_be_bytes(bytes.try_into().unwrap());
    let timestamp = Timestamp(num as i64 - EPOCH);

    Ok(timestamp)
}

/// This is a very simple check to verify that the system time is correct.
/// Retry loop is used in case discrepancies are found.
/// If all retries fail, system clock is considered invalid.
/// TODO: 1. Add proxy functionality in order not to leak connections
pub async fn check_clock(peers: &[Url]) -> Result<()> {
    debug!("System clock check started...");
    let mut r = 0;
    while r < RETRIES {
        if let Err(e) = clock_check(peers).await {
            debug!("Error during clock check: {:#?}", e);
            r += 1;
            continue
        };
        break
    }

    debug!("System clock check finished. Retries: {}", r);
    if r == RETRIES {
        return Err(Error::InvalidClock)
    }

    Ok(())
}

async fn clock_check(peers: &[Url]) -> Result<()> {
    // Start elapsed time counter to cover for all requests and processing time
    let requests_start = Timestamp::current_time();
    // Poll one of the peers for their current UTC timestamp
    let peer_time = peer_request(peers).await?;

    // Start elapsed time counter to cover for NTP request and processing time
    let ntp_request_start = Timestamp::current_time();
    // Poll ntp.org for current timestamp
    let mut ntp_time = ntp_request().await?;

    // Stop elapsed time counters
    let ntp_elapsed_time = ntp_request_start.elapsed() as i64;
    let requests_elapsed_time = requests_start.elapsed() as i64;

    // Current system time
    let system_time = Timestamp::current_time();

    // Add elapsed time to response times
    ntp_time.add(ntp_elapsed_time);
    let peer_time = match peer_time {
        None => None,
        Some(p) => {
            let mut t = p;
            t.add(requests_elapsed_time);
            Some(t)
        }
    };

    debug!("peer_time: {:#?}", peer_time);
    debug!("ntp_time: {:#?}", ntp_time);
    debug!("system_time: {:#?}", system_time);

    // We verify that system time is equal to peer (if exists) and ntp times
    let check = match peer_time {
        Some(p) => (system_time == p) && (system_time == ntp_time),
        None => system_time == ntp_time,
    };

    match check {
        true => Ok(()),
        false => Err(Error::InvalidClock),
    }
}
