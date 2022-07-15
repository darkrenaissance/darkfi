use group::ff::PrimeFieldBits;
use halo2_proofs::{
    arithmetic::FieldExt,
    circuit::{AssignedCell, Chip, Layouter, Region, Value},
    plonk::{Advice, Column, ConstraintSystem, Error, Expression, Selector, TableColumn},
    poly::Rotation,
};
use std::marker::PhantomData;

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
        k_values_table: TableColumn,
    ) -> LessThanConfig {
        let s_lt = meta.selector();

        // configure range check for `a` and `offset`
        let range_a_config = RangeCheckChip::<F, WINDOW_SIZE>::configure(meta, k_values_table);
        let range_a_offset_config =
            RangeCheckChip::<F, WINDOW_SIZE>::configure(meta, k_values_table);

        let config = LessThanConfig { s_lt, a, b, a_offset, range_a_config, range_a_offset_config };

        meta.create_gate("a_offset - 2^m + b - a", |meta| {
            let s_lt = meta.query_selector(config.s_lt);
            let a = meta.query_advice(config.a, Rotation::cur());
            let b = meta.query_advice(config.b, Rotation::cur());
            let a_offset = meta.query_advice(config.a_offset, Rotation::cur());
            let two_pow_m = Expression::Constant(F::from(1 << NUM_OF_BITS));
            // a_offset - 2^m + b - a = 0
            vec![s_lt * (a_offset - two_pow_m + b - a)]
        });

        config
    }

    pub fn witness_less_than(
        &self,
        layouter: &mut impl Layouter<F>,
        a: Value<F>,
        b: Value<F>,
        offset: usize,
    ) -> Result<(), Error> {
        layouter.assign_region(
            || "less than",
            |mut region: Region<'_, F>| {
                let a = region.assign_advice(|| "a", self.config.a, offset, || a)?;
                let b = region.assign_advice(|| "b", self.config.b, offset, || b)?;
                self.less_than(region, a, b, offset)?;
                Ok(())
            },
        )
    }

    pub fn copy_less_than(
        &self,
        layouter: &mut impl Layouter<F>,
        a: AssignedCell<F, F>,
        b: AssignedCell<F, F>,
        offset: usize,
    ) -> Result<(), Error> {
        layouter.assign_region(
            || "less than",
            |mut region: Region<'_, F>| {
                let a = a.copy_advice(|| "a", &mut region, self.config.a, offset)?;
                let b = b.copy_advice(|| "b", &mut region, self.config.b, offset)?;
                self.less_than(region, a, b, offset)?;
                Ok(())
            },
        )
    }

    pub fn less_than(
        &self,
        mut region: Region<'_, F>,
        a: AssignedCell<F, F>,
        b: AssignedCell<F, F>,
        offset: usize,
    ) -> Result<(), Error> {
        // enable `less_than` selector
        self.config.s_lt.enable(&mut region, offset)?;

        // assign `a + offset`
        let two_pow_m = F::from(1 << NUM_OF_BITS);
        let a_offset = a.value().zip(b.value()).map(|(a, b)| *a + (two_pow_m - b));
        let _ = region.assign_advice(|| "offset", self.config.a_offset, offset, || a_offset)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use group::ff::PrimeFieldBits;
    use halo2_proofs::{
        arithmetic::FieldExt,
        circuit::{SimpleFloorPlanner, Value},
        dev::MockProver,
        plonk::Circuit,
    };
    use pasta_curves::pallas;

    struct MyCircuit<
        F: FieldExt + PrimeFieldBits,
        const WINDOW_SIZE: usize,
        const NUM_OF_BITS: usize,
        const NUM_OF_WINDOWS: usize,
    > {
        a: Value<F>,
        b: Value<F>,
    }

    impl<
            F: FieldExt + PrimeFieldBits,
            const WINDOW_SIZE: usize,
            const NUM_OF_BITS: usize,
            const NUM_OF_WINDOWS: usize,
        > Circuit<F> for MyCircuit<F, WINDOW_SIZE, NUM_OF_BITS, NUM_OF_WINDOWS>
    {
        type Config = LessThanConfig;
        type FloorPlanner = SimpleFloorPlanner;

        fn without_witnesses(&self) -> Self {
            Self { a: Value::unknown(), b: Value::unknown() }
        }

        fn configure(meta: &mut ConstraintSystem<F>) -> Self::Config {
            let a = meta.advice_column();
            let b = meta.advice_column();
            let a_offset = meta.advice_column();

            let k_values_table = meta.lookup_table_column();

            let constants = meta.fixed_column();
            meta.enable_constant(constants);

            LessThanChip::<F, NUM_OF_BITS, WINDOW_SIZE, NUM_OF_WINDOWS>::configure(
                meta,
                a,
                b,
                a_offset,
                k_values_table,
            )
        }

        fn synthesize(
            &self,
            config: Self::Config,
            mut layouter: impl Layouter<F>,
        ) -> Result<(), Error> {
            let less_than_chip =
                LessThanChip::<F, NUM_OF_BITS, WINDOW_SIZE, NUM_OF_BITS>::construct(config);

            less_than_chip.witness_less_than(&mut layouter, self.a, self.b, 0)?;

            Ok(())
        }
    }

    #[test]
    fn test_a_b_128_bits() {
        let a = pallas::Base::from_u128(rand::random::<u128>());
        let b = a + pallas::Base::from_u128(rand::random::<u128>());
        let circuit =
            MyCircuit::<pallas::Base, 10, 253, 26> { a: Value::known(a), b: Value::known(b) };
        let prover = MockProver::run(8, &circuit, vec![]).unwrap();
        assert_eq!(prover.verify(), Ok(()));
    }
}
