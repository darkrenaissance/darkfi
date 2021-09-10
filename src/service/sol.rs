use crate::Result;

use super::bridge::CoinClient;

use async_trait::async_trait;

pub struct SolClient{}


impl SolClient {
    pub fn new() -> Result<Self> {
        // Not implemented
        Ok(Self{})
    }
}


#[async_trait]
impl CoinClient for SolClient {
    async fn watch(&self) -> Result<(Vec<u8>, Vec<u8>)> {
        // Not implemented
        Ok((vec![], vec![]))
    }
    async fn send(&self, _address: Vec<u8>, _amount: u64) -> Result<()> {
        // Not implemented
        Ok(())
    }
}
