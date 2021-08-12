use pasta_curves::pallas;
use poseidon::Pow5T3Chip as PoseidonChip;

pub mod poseidon;
pub mod utilities;

impl super::Config {
    pub(super) fn poseidon_chip(&self) -> PoseidonChip<pallas::Base> {
        PoseidonChip::construct(self.poseidon_config.clone())
    }
}
