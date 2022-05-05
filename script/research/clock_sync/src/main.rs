use serde_json::Value;
use std::{
    error::Error,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

// This is a very simple check to verify that system time is correct.
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Poll worldtimeapi.org for current UTC timestamp
    let response =
        reqwest::get("https://worldtimeapi.org/api/timezone/Etc/UTC").await?.text().await?;
    let worldtimeapi: Value = serde_json::from_str(&response).unwrap();
    println!("worldtimeapi: {}", worldtimeapi["unixtime"].to_string());

    // Poll ntp.org for current timestamp
    let address = "0.pool.ntp.org:123";
    let response: ntp::packet::Packet = ntp::request(address).unwrap();
    // Remove 1900 epoch(2208988800) to reach UTC timestamp
    let ntp_time = response.transmit_time.sec as u64 - 2208988800;
    println!("ntp_time: {}", ntp_time);

    // To simulate wrong clock, we sleep some time
    //let one_sec = Duration::new(1, 0);
    //thread::sleep(one_sec);

    // Current system time
    let system_time = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    println!("SystemTime: {}", system_time);

    // We verify that system time is equal to worldtimeapi or ntp
    let check = (system_time == worldtimeapi) || (system_time == ntp_time);
    assert!(check, "System clock is not correct!");

    Ok(())
}
