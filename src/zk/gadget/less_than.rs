use std::marker::PhantomData;

use group::ff::PrimeFieldBits;
use halo2_proofs::{
    arithmetic::FieldExt,
    circuit::Chip,
    plonk::{Advice, Column, ConstraintSystem, Selector},
    poly::Rotation,
};

use super::range_check::{RangeCheckChip, RangeCheckConfig};

#[derive(Clone, Debug)]
pub struct LessThanConfig {
    pub s_lt: Selector,
    pub a: Column<Advice>,
    pub b: Column<Advice>,
    pub a_offset: Column<Advice>,
    pub range_a_config: RangeCheckConfig,
    pub range_a_offset_config: RangeCheckConfig,
}

#[derive(Clone, Debug)]
pub struct LessThanChip<
    F: FieldExt + PrimeFieldBits,
    const NUM_OF_BITS: usize,
    const WINDOW_SIZE: usize,
    const NUM_OF_WINDOWS: usize,
> {
    config: LessThanConfig,
    _marker: PhantomData<F>,
}

impl<
        F: FieldExt + PrimeFieldBits,
        const NUM_OF_BITS: usize,
        const WINDOW_SIZE: usize,
        const NUM_OF_WINDOWS: usize,
    > Chip<F> for LessThanChip<F, NUM_OF_BITS, WINDOW_SIZE, NUM_OF_WINDOWS>
{
    type Config = LessThanConfig;
    type Loaded = ();

    fn config(&self) -> &Self::Config {
        &self.config
    }

    fn loaded(&self) -> &Self::Loaded {
        &()
    }
}

impl<
        F: FieldExt + PrimeFieldBits,
        const NUM_OF_BITS: usize,
        const WINDOW_SIZE: usize,
        const NUM_OF_WINDOWS: usize,
    > LessThanChip<F, NUM_OF_BITS, WINDOW_SIZE, NUM_OF_WINDOWS>
{
    pub fn construct(config: LessThanConfig) -> Self {
        Self { config, _marker: PhantomData }
    }

    pub fn configure(
        meta: &mut ConstraintSystem<F>,
        a: Column<Advice>,
        b: Column<Advice>,
        a_offset: Column<Advice>,
    ) -> LessThanConfig {
        let s_lt = meta.selector();

        // configure range check for `a` and `offset`
        let k_values_table = meta.lookup_table_column();
        let range_a_config = RangeCheckChip::<F, WINDOW_SIZE>::configure(meta, k_values_table);
        let range_a_offset_config =
            RangeCheckChip::<F, WINDOW_SIZE>::configure(meta, k_values_table);

        let config = LessThanConfig { s_lt, a, b, a_offset, range_a_config, range_a_offset_config };

        meta.create_gate("a_offset - 2^m + b - a", |meta| {
            let s_lt = meta.query_selector(config.s_lt);
            let a = meta.query_advice(config.a, Rotation::cur());
            let b = meta.query_advice(config.b, Rotation::cur());
            let a_offset = meta.query_advice(config.a_offset, Rotation::cur());

            // a_offset - 2^m + b - a = 0
            vec![s_lt * (a_offset + b - a)]
        });

        config
    }
}
