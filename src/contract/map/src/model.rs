use darkfi_sdk::pasta::pallas;

use darkfi_serial::{
    SerialDecodable, 
    SerialEncodable
};

#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct SetParamsV1 {
    pub account: pallas::Base,
    pub lock:    pallas::Base,
    pub car:     pallas::Base,
    pub key:     pallas::Base,
    pub value:   pallas::Base,
}

impl SetParamsV1 {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.account,
            self.lock,
            self.car,
            self.key,
            self.value,
        ]
    }
}

#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct SetUpdateV1 {
    pub slot:  pallas::Base,
    pub lock:  pallas::Base,
    pub value: pallas::Base,
}

