use std::path::PathBuf;

use crate::serial::{deserialize, serialize, Decodable, Encodable};
use crate::Result;

pub trait WalletApi {
    fn get_password(&self) -> String;
    fn get_path(&self) -> PathBuf;

    fn get_value_serialized<T: Encodable>(&self, data: &T) -> Result<Vec<u8>> {
        let v = serialize(data);
        Ok(v)
    }

    fn get_value_deserialized<D: Decodable>(&self, key: Vec<u8>) -> Result<D> {
        let v: D = deserialize(&key)?;
        Ok(v)
    }
}
