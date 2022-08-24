use log::error;
use smol::Timer;
use std::time::Duration;

/// Sleep for any number of seconds.
pub async fn sleep(seconds: u64) {
    Timer::after(Duration::from_secs(seconds)).await;
}

/// Auxillary function to reduce boilerplate of sending
/// a message to an optional channel, to notify caller.
pub fn notify_caller(signal: Option<async_channel::Sender<()>>) {
    if let Some(sender) = signal.clone() {
        if let Err(err) = sender.try_send(()) {
            error!(target: "net", "Init signal send error: {}", err);
        }
    }
}
