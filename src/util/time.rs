use std::time::SystemTime;

use async_std::{
    io::{ReadExt, WriteExt},
    net::TcpStream,
};
use chrono::{NaiveDateTime, Utc};
use log::debug;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
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
        let date = timestamp_to_date(self.0, "datetime");
        write!(f, "{}", date)
    }
}

// Clock sync parameters
const RETRIES: u8 = 5;
const WORLDTIMEAPI_ADDRESS: &str = "worldtimeapi.org";
const WORLDTIMEAPI_ADDRESS_WITH_PORT: &str = "worldtimeapi.org:443";
const WORLDTIMEAPI_PAYLOAD: &[u8; 88] = b"GET /api/timezone/Etc/UTC HTTP/1.1\r\nHost: worldtimeapi.org\r\nAccept: application/json\r\n\r\n";
const NTP_ADDRESS: &str = "0.pool.ntp.org:123";
const EPOCH: i64 = 2208988800; //1900

// Raw https request execution for worldtimeapi
async fn worldtimeapi_request() -> Result<Value> {
    // Create connection
    let stream = TcpStream::connect(WORLDTIMEAPI_ADDRESS_WITH_PORT).await?;
    let mut stream = async_native_tls::connect(WORLDTIMEAPI_ADDRESS, stream).await?;
    stream.write_all(WORLDTIMEAPI_PAYLOAD).await?;

    // Execute request
    let mut res = vec![0_u8; 1024];
    stream.read(&mut res).await?;

    // Parse response
    let reply = String::from_utf8(res)?;
    let lines = reply.split('\n');
    // JSON data exist in last row of response
    let last = lines.last().unwrap().trim_matches(char::from(0));
    debug!("worldtimeapi json response: {:#?}", last);
    let reply = serde_json::from_str(last)?;

    Ok(reply)
}

// This is a very simple check to verify that system time is correct.
// Retry loop is used to in case discrepancies are found.
// If all retries fail, system clock is considered invalid.
// TODO: 1. Add proxy functionality in order not to leak connections
//       2. Improve requests and/or add extra protocols
pub async fn check_clock() -> Result<()> {
    debug!("System clock check started...");
    let mut r = 0;
    while r < RETRIES {
        if let Err(e) = clock_check().await {
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

async fn clock_check() -> Result<()> {
    // Start elapsed time counter to cover for all requests and processing time
    let requests_start = Timestamp::current_time();
    // Poll worldtimeapi.org for current UTC timestamp
    let worldtimeapi_response = worldtimeapi_request().await?;

    // Start elapsed time counter to cover for ntp request and processing time
    let ntp_request_start = Timestamp::current_time();
    // Poll ntp.org for current timestamp
    let ntp_response: ntp::packet::Packet = ntp::request(NTP_ADDRESS)?;

    // Extract worldtimeapi timestamp from json
    let mut worldtimeapi_time = Timestamp(worldtimeapi_response["unixtime"].as_i64().unwrap());

    // Remove 1900 epoch to reach UTC timestamp for ntp timestamp
    let mut ntp_time = Timestamp(ntp_response.transmit_time.sec as i64 - EPOCH);

    // Add elapsed time to respone times
    ntp_time.add(ntp_request_start.elapsed() as i64);
    worldtimeapi_time.add(requests_start.elapsed() as i64);

    // Current system time
    let system_time = Timestamp::current_time();

    debug!("worldtimeapi_time: {:#?}", worldtimeapi_time);
    debug!("ntp_time: {:#?}", ntp_time);
    debug!("system_time: {:#?}", system_time);

    // We verify that system time is equal to worldtimeapi and ntp
    let check = (system_time == worldtimeapi_time) && (system_time == ntp_time);
    match check {
        true => Ok(()),
        false => Err(Error::InvalidClock),
    }
}

pub fn timestamp_to_date(timestamp: i64, dt: &str) -> String {
    if timestamp <= 0 {
        return "".to_string()
    }

    match dt {
        "date" => {
            NaiveDateTime::from_timestamp(timestamp, 0).date().format("%A %-d %B").to_string()
        }
        "datetime" => {
            NaiveDateTime::from_timestamp(timestamp, 0).format("%H:%M %A %-d %B").to_string()
        }
        _ => "".to_string(),
    }
}

pub fn unix_timestamp() -> Result<u64> {
    Ok(SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs())
}
