use lazy_static::lazy_static;
use pasta_curves::{group::ff::Field, pallas};
use rand::rngs::OsRng;

pub mod validate;
pub mod wallet;

lazy_static! {
    pub static ref FUNC_ID: pallas::Base = pallas::Base::random(&mut OsRng);
}
