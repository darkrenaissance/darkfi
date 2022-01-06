use smol::Timer;
use std::time::Duration;

/// Sleep for any number of seconds.
pub async fn sleep(seconds: u32) {
    Timer::after(Duration::from_secs(seconds.into())).await;
}
