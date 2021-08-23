use halo2::{
    pasta::pallas,
    plonk::{Advice, Column, Instance as InstanceColumn, Selector},
};

use halo2_ecc::chip::EccConfig;
use halo2_poseidon::pow5t3::Pow5T3Config as PoseidonConfig;

#[derive(Clone, Debug)]
pub struct Config {
    pub primary: Column<InstanceColumn>,
    pub q_add: Selector,
    pub advices: [Column<Advice>; 10],
    pub ecc_config: EccConfig,
    pub poseidon_config: PoseidonConfig<pallas::Base>,
}
