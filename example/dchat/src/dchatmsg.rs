use darkfi::{
    net,
    util::serial::{SerialDecodable, SerialEncodable},
};

impl net::Message for Dchatmsg {
    fn name() -> &'static str {
        "Dchatmsg"
    }
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Dchatmsg {
    pub message: String,
}
