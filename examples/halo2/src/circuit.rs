pub mod gadget;

use gadget::poseidon::Pow5T3Config as PoseidonConfig;
use pasta_curves::pallas;

#[derive(Clone, Debug)]
pub struct Config {
    poseidon_config: PoseidonConfig<pallas::Base>,
}
