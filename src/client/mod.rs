pub mod client;

pub use client::{Client, State};

use std::fmt;

#[derive(Debug)]
pub enum ClientFailed {
    NotEnoughValue(u64),
    UnvalidAddress(String),
    UnvalidAmount(u64),
    UnableToGetDepositAddress,
    UnableToGetWithdrawAddress,
    DoNotHaveCashierPublicKey,
    DoNotHavePublicKey,
    DoNotHavePrivateKey,
    EmptyPassword,
    ClientError(String),
}

impl std::error::Error for ClientFailed {}

impl fmt::Display for ClientFailed {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        match self {
            ClientFailed::NotEnoughValue(i) => {
                write!(f, "There is no enough value {}", i)
            }
            ClientFailed::UnvalidAddress(i) => {
                write!(f, "Unvalid Address {}", i)
            }
            ClientFailed::UnvalidAmount(i) => {
                write!(f, "Unvalid Amount {}", i)
            }
            ClientFailed::UnableToGetDepositAddress => f.write_str("Unable to get deposit address"),
            ClientFailed::UnableToGetWithdrawAddress => {
                f.write_str("Unable to get withdraw address")
            }
            ClientFailed::DoNotHaveCashierPublicKey => f.write_str("Don't have cashier public key"),
            ClientFailed::DoNotHavePublicKey => f.write_str("Don't have public key"),
            ClientFailed::DoNotHavePrivateKey => f.write_str("Don't have private key"),
            ClientFailed::EmptyPassword => f.write_str("Password is empty. Cannot create database"),
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
