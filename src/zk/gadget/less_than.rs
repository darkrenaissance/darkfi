use halo2_proofs::{
    arithmetic::FieldExt,
    circuit::{AssignedCell, Chip, Layouter, Region, Value},
    pasta::pallas,
    plonk::{Advice, Column, ConstraintSystem, Error, Expression, Selector, TableColumn},
    poly::Rotation,
};

use super::native_range_check::{NativeRangeCheckChip, NativeRangeCheckConfig};

#[derive(Clone, Debug)]
pub struct LessThanConfig<
    const WINDOW_SIZE: usize,
    const NUM_OF_BITS: usize,
    const NUM_OF_WINDOWS: usize,
> {
    pub s_lt: Selector,
    pub a: Column<Advice>,
    pub b: Column<Advice>,
    pub a_offset: Column<Advice>,
    pub range_a_config: NativeRangeCheckConfig<WINDOW_SIZE, NUM_OF_BITS, NUM_OF_WINDOWS>,
    pub range_a_offset_config: NativeRangeCheckConfig<WINDOW_SIZE, NUM_OF_BITS, NUM_OF_WINDOWS>,
    pub k_values_table: TableColumn,
}

#[derive(Clone, Debug)]
pub struct LessThanChip<
    const WINDOW_SIZE: usize,
    const NUM_OF_BITS: usize,
    const NUM_OF_WINDOWS: usize,
> {
    config: LessThanConfig<WINDOW_SIZE, NUM_OF_BITS, NUM_OF_WINDOWS>,
}

impl<const WINDOW_SIZE: usize, const NUM_OF_BITS: usize, const NUM_OF_WINDOWS: usize>
    Chip<pallas::Base> for LessThanChip<WINDOW_SIZE, NUM_OF_BITS, NUM_OF_WINDOWS>
{
    type Config = LessThanConfig<WINDOW_SIZE, NUM_OF_BITS, NUM_OF_WINDOWS>;
    type Loaded = ();

    fn config(&self) -> &Self::Config {
        &self.config
    }

    fn loaded(&self) -> &Self::Loaded {
        &()
    }
}

impl<const WINDOW_SIZE: usize, const NUM_OF_BITS: usize, const NUM_OF_WINDOWS: usize>
    LessThanChip<WINDOW_SIZE, NUM_OF_BITS, NUM_OF_WINDOWS>
{
    pub fn construct(config: LessThanConfig<WINDOW_SIZE, NUM_OF_BITS, NUM_OF_WINDOWS>) -> Self {
        Self { config }
    }

    pub fn configure(
        meta: &mut ConstraintSystem<pallas::Base>,
        a: Column<Advice>,
        b: Column<Advice>,
        a_offset: Column<Advice>,
        k_values_table: TableColumn,
    ) -> LessThanConfig<WINDOW_SIZE, NUM_OF_BITS, NUM_OF_WINDOWS> {
        let s_lt = meta.selector();

        meta.enable_equality(a);
        meta.enable_equality(b);
        meta.enable_equality(a_offset);

        // configure range check for `a` and `offset`
        let z = meta.advice_column();
        let range_a_config =
            NativeRangeCheckChip::<WINDOW_SIZE, NUM_OF_BITS, NUM_OF_WINDOWS>::configure(
                meta,
                z,
                k_values_table,
            );
        let z = meta.advice_column();
        let range_a_offset_config =
            NativeRangeCheckChip::<WINDOW_SIZE, NUM_OF_BITS, NUM_OF_WINDOWS>::configure(
                meta,
                z,
                k_values_table,
            );

        let config = LessThanConfig {
            s_lt,
            a,
            b,
            a_offset,
            range_a_config,
            range_a_offset_config,
            k_values_table,
        };

        meta.create_gate("a_offset - 2^m + b - a", |meta| {
            let s_lt = meta.query_selector(config.s_lt);
            let a = meta.query_advice(config.a, Rotation::cur());
            let b = meta.query_advice(config.b, Rotation::cur());
            let a_offset = meta.query_advice(config.a_offset, Rotation::cur());
            let two_pow_m =
                Expression::Constant(pallas::Base::from(2).pow(&[NUM_OF_BITS as u64, 0, 0, 0]));
            // a_offset - 2^m + b - a = 0
            vec![s_lt * (a_offset - two_pow_m + b - a)]
        });

        config
    }

    pub fn witness_less_than(
        &self,
        mut layouter: impl Layouter<pallas::Base>,
        a: Value<pallas::Base>,
        b: Value<pallas::Base>,
        offset: usize,
    ) -> Result<(), Error> {
        let (a, _, a_offset) = layouter.assign_region(
            || "a less than b",
            |mut region: Region<'_, pallas::Base>| {
                let a = region.assign_advice(|| "a", self.config.a, offset, || a)?;
                let b = region.assign_advice(|| "b", self.config.b, offset, || b)?;
                let a_offset = self.less_than(region, a.clone(), b.clone(), offset)?;
                Ok((a, b, a_offset))
            },
        )?;

        self.less_than_range_check(layouter, a, a_offset)?;

        Ok(())
    }

    pub fn copy_less_than(
        &self,
        mut layouter: impl Layouter<pallas::Base>,
        a: AssignedCell<pallas::Base, pallas::Base>,
        b: AssignedCell<pallas::Base, pallas::Base>,
        offset: usize,
    ) -> Result<(), Error> {
        let (a, _, a_offset) = layouter.assign_region(
            || "a less than b",
            |mut region: Region<'_, pallas::Base>| {
                let a = a.copy_advice(|| "a", &mut region, self.config.a, offset)?;
                let b = b.copy_advice(|| "b", &mut region, self.config.b, offset)?;
                let a_offset = self.less_than(region, a.clone(), b.clone(), offset)?;
                Ok((a, b, a_offset))
            },
        )?;

        self.less_than_range_check(layouter, a, a_offset)?;

        Ok(())
    }

    pub fn less_than_range_check(
        &self,
        mut layouter: impl Layouter<pallas::Base>,
        a: AssignedCell<pallas::Base, pallas::Base>,
        a_offset: AssignedCell<pallas::Base, pallas::Base>,
    ) -> Result<(), Error> {
        let range_a_chip =
            NativeRangeCheckChip::<WINDOW_SIZE, NUM_OF_BITS, NUM_OF_WINDOWS>::construct(
                self.config.range_a_config.clone(),
            );
        let range_a_offset_chip =
            NativeRangeCheckChip::<WINDOW_SIZE, NUM_OF_BITS, NUM_OF_WINDOWS>::construct(
                self.config.range_a_offset_config.clone(),
            );

        range_a_chip.copy_range_check(layouter.namespace(|| "a copy_range_check"), a)?;
        range_a_offset_chip
            .copy_range_check(layouter.namespace(|| "a_offset copy_range_check"), a_offset)?;

        Ok(())
    }

    pub fn less_than(
        &self,
        mut region: Region<'_, pallas::Base>,
        a: AssignedCell<pallas::Base, pallas::Base>,
        b: AssignedCell<pallas::Base, pallas::Base>,
        offset: usize,
    ) -> Result<AssignedCell<pallas::Base, pallas::Base>, Error> {
        // enable `less_than` selector
        self.config.s_lt.enable(&mut region, offset)?;

        // assign `a + offset`
        let two_pow_m = pallas::Base::from(2).pow(&[NUM_OF_BITS as u64, 0, 0, 0]);
        let a_offset = a.value().zip(b.value()).map(|(a, b)| *a + (two_pow_m - b));
        let a_offset =
            region.assign_advice(|| "a_offset", self.config.a_offset, offset, || a_offset)?;

        Ok(a_offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use halo2_proofs::{
        circuit::{floor_planner, Value},
        dev::{CircuitLayout, MockProver},
        pasta::pallas,
        plonk::Circuit,
    };

    macro_rules! test_circuit {
        ($window_size:expr, $num_bits:expr, $num_windows:expr) => {
            #[derive(Default)]
            struct LessThanCircuit {
                a: Value<pallas::Base>,
                b: Value<pallas::Base>,
            }

            impl Circuit<pallas::Base> for LessThanCircuit {
                type Config =
                    (LessThanConfig<$window_size, $num_bits, $num_windows>, Column<Advice>);
                type FloorPlanner = floor_planner::V1;

                fn without_witnesses(&self) -> Self {
                    Self { a: Value::unknown(), b: Value::unknown() }
                }

                fn configure(meta: &mut ConstraintSystem<pallas::Base>) -> Self::Config {
                    let w = meta.advice_column();
                    meta.enable_equality(w);

                    let a = meta.advice_column();
                    let b = meta.advice_column();
                    let a_offset = meta.advice_column();

                    let k_values_table = meta.lookup_table_column();

                    let constants = meta.fixed_column();
                    meta.enable_constant(constants);

                    (
                        LessThanChip::<$window_size, $num_bits, $num_windows>::configure(
                            meta,
                            a,
                            b,
                            a_offset,
                            k_values_table,
                        ),
                        w,
                    )
                }

                fn synthesize(
                    &self,
                    config: Self::Config,
                    mut layouter: impl Layouter<pallas::Base>,
                ) -> Result<(), Error> {
                    let less_than_chip =
                        LessThanChip::<$window_size, $num_bits, $num_windows>::construct(
                            config.0.clone(),
                        );

                    NativeRangeCheckChip::<$window_size, $num_bits, $num_windows>::load_k_table(
                        &mut layouter,
                        config.0.k_values_table,
                    )?;

                    less_than_chip.witness_less_than(
                        layouter.namespace(|| "a < b"),
                        self.a,
                        self.b,
                        0,
                    )?;

                    Ok(())
                }
            }
        };
    }

    #[test]
    fn less_than_64() {
        test_circuit!(3, 64, 22);
        let k = 5;

        let valid_a_vals = vec![pallas::Base::from(13), pallas::Base::zero(), pallas::Base::one()];
        let valid_b_vals = vec![
            pallas::Base::from(15),
            pallas::Base::from(u64::MAX),
            pallas::Base::from(rand::random::<u64>()),
        ];

        let invalid_a_vals = vec![
            pallas::Base::from(14),
            pallas::Base::from(u64::MAX),
            pallas::Base::zero(),
            pallas::Base::one(),
            pallas::Base::from(u64::MAX),
        ];
        let invalid_b_vals = vec![
            pallas::Base::from(11),
            pallas::Base::zero(),
            pallas::Base::zero(),
            pallas::Base::one(),
            pallas::Base::from(u64::MAX),
        ];

        use plotters::prelude::*;
        let circuit = LessThanCircuit {
            a: Value::known(pallas::Base::zero()),
            b: Value::known(pallas::Base::one()),
        };
        let root = BitMapBackend::new("target/lessthan_circuit_layout.png", (3840, 2160))
            .into_drawing_area();
        CircuitLayout::default().render(k, &circuit, &root).unwrap();

        for i in 0..valid_a_vals.len() {
            let a = valid_a_vals[i];
            let b = valid_b_vals[i];

            println!("64 bit (valid) {:?} < {:?} check", a, b);

            let circuit = LessThanCircuit { a: Value::known(a), b: Value::known(b) };

            let prover = MockProver::run(k, &circuit, vec![]).unwrap();
            prover.assert_satisfied();
        }

        for i in 0..invalid_a_vals.len() {
            let a = invalid_a_vals[i];
            let b = invalid_b_vals[i];

            println!("64 bit (invalid) {:?} < {:?} check", a, b);

            let circuit = LessThanCircuit { a: Value::known(a), b: Value::known(b) };

            let prover = MockProver::run(k, &circuit, vec![]).unwrap();
            assert!(prover.verify().is_err())
        }
    }
}
