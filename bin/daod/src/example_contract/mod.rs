use lazy_static::lazy_static;
use pasta_curves::{group::ff::Field, pallas};
use rand::rngs::OsRng;

// foo()
pub mod foo;

pub mod state;

lazy_static! {
    pub static ref CONTRACT_ID: pallas::Base = pallas::Base::random(&mut OsRng);
}
