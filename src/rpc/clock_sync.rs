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

//! Clock sync module
use std::{net::UdpSocket, time::Duration};

use tracing::debug;
use url::Url;

use crate::{util::time::Timestamp, Error, Result};

/// Clock sync parameters
const RETRIES: u8 = 10;
/// TODO: Loop through set of ntps, get their average response concurrenyly.
const NTP_ADDRESS: &str = "pool.ntp.org:123";
const EPOCH: u32 = 2208988800; // 1900

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
    let timestamp = Timestamp::from_u64((num - EPOCH) as u64);

    Ok(timestamp)
}

/// This is a very simple check to verify that the system time is correct.
///
/// Retry loop is used in case discrepancies are found.
/// If all retries fail, system clock is considered invalid.
/// TODO: 1. Add proxy functionality in order not to leak connections
pub async fn check_clock(peers: &[Url]) -> Result<()> {
    debug!(target: "rpc::clock_sync", "System clock check started...");
    let mut r = 0;
    while r < RETRIES {
        if let Err(e) = clock_check(peers).await {
            debug!(target: "rpc::clock_sync", "Error during clock check: {e:#?}");
            r += 1;
            continue
        };
        break
    }

    debug!(target: "rpc::clock_sync", "System clock check finished. Retries: {r}");
    if r == RETRIES {
        return Err(Error::InvalidClock)
    }

    Ok(())
}

async fn clock_check(_peers: &[Url]) -> Result<()> {
    // Start elapsed time counter to cover for all requests and processing time
    let requests_start = Timestamp::current_time();
    // Poll one of the peers for their current UTC timestamp
    //let peer_time = peer_request(peers).await?;
    let peer_time = Some(Timestamp::current_time());

    // Start elapsed time counter to cover for NTP request and processing time
    let ntp_request_start = Timestamp::current_time();
    // Poll ntp.org for current timestamp
    let ntp_time = ntp_request().await?;

    // Stop elapsed time counters
    let ntp_elapsed_time = ntp_request_start.elapsed()?;
    let requests_elapsed_time = requests_start.elapsed()?;

    // Current system time
    let system_time = Timestamp::current_time();

    // Add elapsed time to response times
    let ntp_time = ntp_time.checked_add(ntp_elapsed_time)?;
    let peer_time = match peer_time {
        None => None,
        Some(p) => Some(p.checked_add(requests_elapsed_time)?),
    };

    debug!(target: "rpc::clock_sync", "peer_time: {peer_time:#?}");
    debug!(target: "rpc::clock_sync", "ntp_time: {ntp_time:#?}");
    debug!(target: "rpc::clock_sync", "system_time: {system_time:#?}");

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
