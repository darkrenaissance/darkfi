#![no_main]
extern crate darkfi_serial;
use darkfi_serial::{deserialize, serialize};

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let ser = serialize(&s);
        let des: String = deserialize(&ser).unwrap();
        assert_eq!(s, des);
    }
});
