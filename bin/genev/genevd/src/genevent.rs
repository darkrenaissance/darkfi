use darkfi::event_graph::EventMsg;
use darkfi_serial::{SerialDecodable, SerialEncodable};

#[derive(SerialEncodable, SerialDecodable, Clone, Debug)]
pub struct GenEvent {
    pub nick: String,
    pub title: String,
    pub text: String,
}

impl EventMsg for GenEvent {
    fn new() -> Self {
        Self {
            nick: "groot".to_string(),
            title: "I am groot".to_string(),
            text: "I am groot!!".to_string(),
        }
    }
}
