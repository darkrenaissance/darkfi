use std::io::{Error, ErrorKind, Read, Write};

use url::Url;

use crate::{Decodable, Encodable};

impl Encodable for Url {
    #[inline]
    fn encode<S: Write>(&self, s: S) -> Result<usize, Error> {
        self.as_str().to_string().encode(s)
    }
}

impl Decodable for Url {
    #[inline]
    fn decode<D: Read>(mut d: D) -> Result<Self, Error> {
        let s: String = Decodable::decode(&mut d)?;
        match Url::parse(&s) {
            Ok(v) => Ok(v),
            Err(e) => Err(Error::new(ErrorKind::Other, e)),
        }
    }
}
