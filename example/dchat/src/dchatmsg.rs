use async_std::sync::{Arc, Mutex};

use darkfi::{
    net,
    serial::{SerialDecodable, SerialEncodable},
};

pub type DchatMsgsBuffer = Arc<Mutex<Vec<DchatMsg>>>;

impl net::Message for DchatMsg {
    fn name() -> &'static str {
        "DchatMsg"
    }
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DchatMsg {
    pub msg: String,
}
