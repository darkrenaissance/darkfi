use lazy_static::lazy_static;
use pasta_curves::{group::ff::Field, pallas};
use rand::rngs::OsRng;

// mint()
pub mod mint;
// propose()
pub mod propose;
// vote{}
pub mod vote;
// exec{}
pub mod exec;

pub mod state;

pub use state::{DaoBulla, State};

lazy_static! {
    pub static ref CONTRACT_ID: pallas::Base = pallas::Base::random(&mut OsRng);
}
