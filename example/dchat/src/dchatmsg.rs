use async_std::sync::{Arc, Mutex};
use darkfi::{
    net,
    util::serial::{SerialDecodable, SerialEncodable},
};
use ringbuffer::AllocRingBuffer;

impl net::Message for Dchatmsg {
    fn name() -> &'static str {
        "Dchatmsg"
    }
}

pub type DchatmsgsBuffer = Arc<Mutex<AllocRingBuffer<Dchatmsg>>>;

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Dchatmsg {
    pub message: String,
}
