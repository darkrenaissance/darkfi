use pasta_curves::pallas;

use ecc::chip::EccChip;
use poseidon::Pow5T3Chip as PoseidonChip;

pub mod ecc;
pub mod poseidon;
pub mod utilities;

impl super::Config {
    pub(super) fn ecc_chip(&self) -> EccChip {
        EccChip::construct(self.ecc_config.clone())
    }

    pub(super) fn poseidon_chip(&self) -> PoseidonChip<pallas::Base> {
        PoseidonChip::construct(self.poseidon_config.clone())
    }
}
