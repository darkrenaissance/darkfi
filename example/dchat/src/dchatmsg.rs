use async_std::sync::{Arc, Mutex};

use darkfi::{
    net,
    util::serial::{SerialDecodable, SerialEncodable},
};

pub type DchatmsgsBuffer = Arc<Mutex<Vec<Dchatmsg>>>;

impl net::Message for Dchatmsg {
    fn name() -> &'static str {
        "Dchatmsg"
    }
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Dchatmsg {
    pub msg: String,
}
