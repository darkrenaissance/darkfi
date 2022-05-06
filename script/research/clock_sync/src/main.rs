use async_std::{
    io::{ReadExt, WriteExt},
    net::TcpStream,
};
use std::{
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use log::{debug, error, info};
use serde_json::Value;
use simplelog::{ColorChoice, Config, LevelFilter, TermLogger, TerminalMode};

use darkfi::Result;

mod error;

use crate::error::{ClockError, ClockResult};

// Execution parameters
const RETRIES: u8 = 5;
const WORLDTIMEAPI_ADDRESS: &str = "worldtimeapi.org";
const WORLDTIMEAPI_ADDRESS_WITH_PORT: &str = "worldtimeapi.org:443";
const WORLDTIMEAPI_PAYLOAD: &[u8; 88] = b"GET /api/timezone/Etc/UTC HTTP/1.1\r\nHost: worldtimeapi.org\r\nAccept: application/json\r\n\r\n";
const NTP_ADDRESS: &str = "0.pool.ntp.org:123";
const EPOCH: u64 = 2208988800; //1900

// Raw https request execution for worldtimeapi
async fn worldtimeapi_request() -> ClockResult<Value> {
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
    debug!("worldtimeapi json response: {}", last);
    let reply = serde_json::from_str(last)?;
    Ok(reply)
}

// This is a very simple check to verify that system time is correct.
// Retry loop is used to in case discrepancies are found.
// If all retries fail, system clock is considered invalid.
// TODO: 1. Add proxy functionality in order not to leak connections
//       2. Improve requests and/or add extra protocols
async fn check_clock() -> ClockResult<()> {
    debug!("System clock check started...");
    let mut r = 0;
    while r < RETRIES {
        let check = clock_check().await;
        match check {
            Ok(()) => break,
            Err(e) => error!("Error during clock check: {}", e),
        }
        r += 1;
    }

    debug!("System clock check finished. Retries: {}", r);
    match r {
        RETRIES => Err(ClockError::InvalidClock),
        _ => Ok(()),
    }
}

async fn clock_check() -> ClockResult<()> {
    // Start elapsed time counter to cover for all requests and processing time
    let requests_start = Instant::now();
    // Poll worldtimeapi.org for current UTC timestamp
    let worldtimeapi_response = worldtimeapi_request().await?;

    // Start elapsed time counter to cover for ntp request and processing time
    let ntp_request_start = Instant::now();
    // Poll ntp.org for current timestamp
    let ntp_response: ntp::packet::Packet = ntp::request(NTP_ADDRESS)?;

    // Extract worldtimeapi timestamp from json
    let mut worldtimeapi_time = worldtimeapi_response["unixtime"].as_u64().unwrap();

    // Remove 1900 epoch to reach UTC timestamp for ntp timestamp
    let mut ntp_time = ntp_response.transmit_time.sec as u64 - EPOCH;

    // Add elapsed time to respone times
    ntp_time += ntp_request_start.elapsed().as_secs();
    worldtimeapi_time += requests_start.elapsed().as_secs();

    // To simulate wrong clock, we sleep some time
    //let one_sec = Duration::new(1, 0);
    //thread::sleep(one_sec);

    // Current system time
    let system_time = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

    debug!("worldtimeapi_time: {}", worldtimeapi_time);
    debug!("ntp_time: {}", ntp_time);
    debug!("system_time: {}", system_time);

    // We verify that system time is equal to worldtimeapi or ntp
    let check = (system_time == worldtimeapi_time) && (system_time == ntp_time);
    match check {
        true => Ok(()),
        false => Err(ClockError::InvalidClock),
    }
}

#[async_std::main]
async fn main() -> Result<()> {
    TermLogger::init(
        LevelFilter::Debug,
        Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )?;

    match check_clock().await {
        Ok(()) => info!("System clock is correct!"),
        Err(_) => {
            error!("System clock is invalid, terminating...");
            return Err(darkfi::Error::OperationFailed)
        }
    };

    Ok(())
}
