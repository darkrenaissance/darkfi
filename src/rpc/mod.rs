pub mod adapter;
pub mod jsonserver;
pub mod test;

use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct TransferParams {
    pub address: String,
    pub amount: String,
}

#[derive(Deserialize, Debug)]
pub struct WithdrawParams {
    address: String,
    amount: String,
}

pub use adapter::{AdapterPtr, RpcAdapter};
