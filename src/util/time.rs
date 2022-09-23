use std::{
    mem,
    net::UdpSocket,
    time::{Duration, UNIX_EPOCH},
};

use chrono::{NaiveDateTime, Utc};
use log::debug;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use serde_json::json;
use url::Url;

use crate::{
    rpc::{client::RpcClient, jsonrpc::JsonRequest},
    util::serial::{SerialDecodable, SerialEncodable},
    Error, Result,
};

/// Wrapper struct to represent [`chrono`] UTC timestamps.
#[derive(
    Clone,
    Copy,
    Debug,
    Serialize,
    Deserialize,
    SerialEncodable,
    SerialDecodable,
    PartialEq,
    PartialOrd,
    Eq,
)]
pub struct Timestamp(pub i64);

impl Timestamp {
    /// Generate a `Timestamp` of the current time.
    pub fn current_time() -> Self {
        Self(Utc::now().timestamp())
    }

    /// Calculates elapsed time of a `Timestamp`.
    pub fn elapsed(&self) -> u64 {
        let start_time = NaiveDateTime::from_timestamp(self.0, 0);
        let end_time = NaiveDateTime::from_timestamp(Utc::now().timestamp(), 0);
        let diff = end_time - start_time;
        diff.num_seconds() as u64
    }

    /// Increment a 'Timestamp'.
    pub fn add(&mut self, inc: i64) {
        self.0 += inc;
    }
}

impl std::fmt::Display for Timestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let date = timestamp_to_date(self.0, DateFormat::DateTime);
        write!(f, "{}", date)
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    Serialize,
    Deserialize,
    SerialEncodable,
    SerialDecodable,
    PartialEq,
    PartialOrd,
    Eq,
)]
pub struct NanoTimestamp(pub i64);

impl NanoTimestamp {
    pub fn current_time() -> Self {
        Self(Utc::now().timestamp_nanos())
    }
}
impl std::fmt::Display for NanoTimestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let date = timestamp_to_date(self.0, DateFormat::Nanos);
        write!(f, "{}", date)
    }
}

// Clock sync parameters
const RETRIES: u8 = 10;
///TODO loop through set of ntps, get their average response concurrently.
const NTP_ADDRESS: &str = "pool.ntp.org:123";
const EPOCH: i64 = 2208988800; //1900

// JsonRPC request to a network peer(randomly selected),
// to retrieve their current system clock.
async fn peer_request(peers: &Vec<Url>) -> Result<Option<Timestamp>> {
    // Select peer, None if vector is empty
    let peer = peers.choose(&mut rand::thread_rng());
    match peer {
        None => Ok(None),
        Some(p) => {
            // Create rpc client
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

// Raw ntp request execution
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
    let (bytes, _) = packet[40..44].split_at(mem::size_of::<u32>());
    let num = u32::from_be_bytes(bytes.try_into().unwrap());
    let timestamp = Timestamp(num as i64 - EPOCH);

    Ok(timestamp)
}

// This is a very simple check to verify that system time is correct.
// Retry loop is used to in case discrepancies are found.
// If all retries fail, system clock is considered invalid.
// TODO: 1. Add proxy functionality in order not to leak connections
pub async fn check_clock(peers: Vec<Url>) -> Result<()> {
    debug!("System clock check started...");
    let mut r = 0;
    while r < RETRIES {
        if let Err(e) = clock_check(&peers).await {
            debug!("Error during clock check: {:#?}", e);
            r += 1;
            continue
        };
        break
    }

    debug!("System clock check finished. Retries: {:#?}", r);
    match r {
        RETRIES => Err(Error::InvalidClock),
        _ => Ok(()),
    }
}

async fn clock_check(peers: &Vec<Url>) -> Result<()> {
    // Start elapsed time counter to cover for all requests and processing time
    let requests_start = Timestamp::current_time();
    // Poll one of peers for their current UTC timestamp
    let peer_time = peer_request(peers).await?;

    // Start elapsed time counter to cover for ntp request and processing time
    let ntp_request_start = Timestamp::current_time();
    // Poll ntp.org for current timestamp
    let mut ntp_time = ntp_request().await?;

    // Stop elapsed time counters
    let ntp_elapsed_time = ntp_request_start.elapsed() as i64;
    let requests_elapsed_time = requests_start.elapsed() as i64;

    // Current system time
    let system_time = Timestamp::current_time();

    // Add elapsed time to respone times
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

    // We verify that system time is equal to peer(if exists) and ntp times
    let check = match peer_time {
        Some(p) => (system_time == p) && (system_time == ntp_time),
        None => system_time == ntp_time,
    };
    match check {
        true => Ok(()),
        false => Err(Error::InvalidClock),
    }
}

pub enum DateFormat {
    Default,
    Date,
    DateTime,
    Nanos,
}

pub fn timestamp_to_date(timestamp: i64, format: DateFormat) -> String {
    if timestamp <= 0 {
        return "".to_string()
    }

    match format {
        DateFormat::Date => {
            NaiveDateTime::from_timestamp(timestamp, 0).date().format("%-d %b").to_string()
        }
        DateFormat::DateTime => {
            NaiveDateTime::from_timestamp(timestamp, 0).format("%H:%M:%S %A %-d %B").to_string()
        }
        DateFormat::Nanos => {
            const A_BILLION: i64 = 1_000_000_000;
            NaiveDateTime::from_timestamp(timestamp / A_BILLION, (timestamp % A_BILLION) as u32)
                .format("%H:%M:%S.%f")
                .to_string()
        }
        DateFormat::Default => "".to_string(),
    }
}

pub fn unix_timestamp() -> Result<u64> {
    Ok(UNIX_EPOCH.elapsed()?.as_secs())
}
