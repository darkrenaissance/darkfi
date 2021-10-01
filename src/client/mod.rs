pub mod client;

pub use client::{Client, State};

use std::fmt;

#[derive(Debug)]
pub enum ClientFailed {
    NotEnoughValue(u64),
    InvalidAddress(String),
    InvalidAmount(u64),
    UnableToGetDepositAddress,
    UnableToGetWithdrawAddress,
    DoesNotHaveCashierPublicKey,
    DoesNotHaveKeypair,
    EmptyPassword,
    WalletInitialized,
    KeyExists,
    ClientError(String),
}

impl std::error::Error for ClientFailed {}

impl fmt::Display for ClientFailed {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        match self {
            ClientFailed::NotEnoughValue(i) => {
                write!(f, "There is no enough value {}", i)
            }
            ClientFailed::InvalidAddress(i) => {
                write!(f, "Invalid Address {}", i)
            }
            ClientFailed::InvalidAmount(i) => {
                write!(f, "Invalid Amount {}", i)
            }
            ClientFailed::UnableToGetDepositAddress => f.write_str("Unable to get deposit address"),
            ClientFailed::UnableToGetWithdrawAddress => {
                f.write_str("Unable to get withdraw address")
            }
            ClientFailed::DoesNotHaveCashierPublicKey => {
                f.write_str("Does not have cashier public key")
            }
            ClientFailed::DoesNotHaveKeypair => f.write_str("Does not have keypair"),
            ClientFailed::EmptyPassword => f.write_str("Password is empty. Cannot create database"),
            ClientFailed::WalletInitialized => f.write_str("Wallet already initalized"),
            ClientFailed::KeyExists => f.write_str("Keypair already exists"),
            ClientFailed::ClientError(i) => {
                write!(f, "ClientError: {}", i)
            }
        }
    }
}

impl From<super::error::Error> for ClientFailed {
    fn from(err: super::error::Error) -> ClientFailed {
        ClientFailed::ClientError(err.to_string())
    }
}

pub type ClientResult<T> = std::result::Result<T, ClientFailed>;
