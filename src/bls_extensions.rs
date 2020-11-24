use bls12_381 as bls;

use std::io;

use crate::error::{Error, Result};
use crate::serial::{Decodable, Encodable, ReadExt, WriteExt};

macro_rules! from_slice {
    ($data:expr, $len:literal) => {{
        let mut array = [0; $len];
        // panics if not enough data
        let bytes = &$data[..array.len()];
        array.copy_from_slice(bytes);
        array
    }};
}

pub trait BlsStringConversion {
    fn to_string(&self) -> String;
    fn from_string(object: &str) -> Self;
}

impl BlsStringConversion for bls::Scalar {
    fn to_string(&self) -> String {
        let mut bytes = self.to_bytes();
        bytes.reverse();
        hex::encode(bytes)
    }
    fn from_string(object: &str) -> Self {
        let mut bytes = from_slice!(&hex::decode(object).unwrap(), 32);
        bytes.reverse();
        bls::Scalar::from_bytes(&bytes).unwrap()
    }
}

macro_rules! serialization_bls {
    ($type:ty, $to_x:ident, $from_x:ident, $size:literal) => {
        impl Encodable for $type {
            fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
                let data = self.$to_x();
                assert_eq!(data.len(), $size);
                s.write_slice(&data)?;
                Ok(data.len())
            }
        }

        impl Decodable for $type {
            fn decode<D: io::Read>(mut d: D) -> Result<Self> {
                let mut slice = [0u8; $size];
                d.read_slice(&mut slice)?;
                let result = Self::$from_x(&slice);
                if bool::from(result.is_none()) {
                    return Err(Error::ParseFailed("$t conversion from slice failed"));
                }
                Ok(result.unwrap())
            }
        }
    };
}

serialization_bls!(bls::Scalar, to_bytes, from_bytes, 32);

macro_rules! make_serialize_deserialize_test {
    ($name:ident, $type:ty, $default_func:ident) => {
        #[test]
        fn $name() {
            let point = <$type>::$default_func();

            let mut data: Vec<u8> = vec![];
            let result = point.encode(&mut data);
            assert!(result.is_ok());

            let point2 = <$type>::decode(&data[..]);
            assert!(point2.is_ok());
            let point2 = point2.unwrap();

            assert_eq!(point, point2);
        }
    };
}

make_serialize_deserialize_test!(serial_test_scalar, bls::Scalar, zero);
