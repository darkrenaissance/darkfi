use group::ff::{Field, PrimeFieldBits};
use halo2_proofs::{
    circuit::{AssignedCell, Chip, Layouter, Region, Value},
    pasta::pallas,
    plonk,
    plonk::{Advice, Column, ConstraintSystem, Selector, TableColumn},
    poly::Rotation,
};

#[derive(Clone, Debug)]
pub struct NativeRangeCheckConfig<
    const WINDOW_SIZE: usize,
    const NUM_BITS: usize,
    const NUM_WINDOWS: usize,
> {
    pub z: Column<Advice>,
    pub s_rc: Selector,
    pub k_values_table: TableColumn,
}

#[derive(Clone, Debug)]
pub struct NativeRangeCheckChip<
    const WINDOW_SIZE: usize,
    const NUM_BITS: usize,
    const NUM_WINDOWS: usize,
> {
    config: NativeRangeCheckConfig<WINDOW_SIZE, NUM_BITS, NUM_WINDOWS>,
}

impl<const WINDOW_SIZE: usize, const NUM_BITS: usize, const NUM_WINDOWS: usize> Chip<pallas::Base>
    for NativeRangeCheckChip<WINDOW_SIZE, NUM_BITS, NUM_WINDOWS>
{
    type Config = NativeRangeCheckConfig<WINDOW_SIZE, NUM_BITS, NUM_WINDOWS>;
    type Loaded = ();

    fn config(&self) -> &Self::Config {
        &self.config
    }

    fn loaded(&self) -> &Self::Loaded {
        &()
    }
}

impl<const WINDOW_SIZE: usize, const NUM_BITS: usize, const NUM_WINDOWS: usize>
    NativeRangeCheckChip<WINDOW_SIZE, NUM_BITS, NUM_WINDOWS>
{
    pub fn construct(config: NativeRangeCheckConfig<WINDOW_SIZE, NUM_BITS, NUM_WINDOWS>) -> Self {
        Self { config }
    }

    pub fn configure(
        meta: &mut ConstraintSystem<pallas::Base>,
        z: Column<Advice>,
        k_values_table: TableColumn,
    ) -> NativeRangeCheckConfig<WINDOW_SIZE, NUM_BITS, NUM_WINDOWS> {
        let s_rc = meta.complex_selector();

        meta.lookup(|meta| {
            let s_rc = meta.query_selector(s_rc);
            let z_curr = meta.query_advice(z, Rotation::cur());
            let z_next = meta.query_advice(z, Rotation::next());

            //    z_next = (z_curr - k_i) / 2^K
            // => k_i = z_curr - (z_next * 2^K)
            vec![(s_rc * (z_curr - z_next * pallas::Base::from(1 << WINDOW_SIZE)), k_values_table)]
        });

        NativeRangeCheckConfig { z, s_rc, k_values_table }
    }

    /// `k_values_table` should be reused across different chips
    /// which is why we don't limit it to a specific instance.
    pub fn load_k_table(
        &self,
        layouter: &mut impl Layouter<pallas::Base>,
        k_values_table: TableColumn,
    ) -> Result<(), plonk::Error> {
        layouter.assign_table(
            || format!("{} window table", WINDOW_SIZE),
            |mut table| {
                for index in 0..(1 << WINDOW_SIZE) {
                    table.assign_cell(
                        || format!("{} window assign", WINDOW_SIZE),
                        k_values_table,
                        index,
                        || Value::known(pallas::Base::from(index as u64)),
                    )?;
                }
                Ok(())
            },
        )
    }

    fn decompose_value(value: &pallas::Base) -> Vec<[bool; WINDOW_SIZE]> {
        let padding = (WINDOW_SIZE - NUM_BITS % WINDOW_SIZE) % WINDOW_SIZE;

        let bits: Vec<bool> = value
            .to_le_bits()
            .into_iter()
            .take(NUM_BITS)
            .chain(std::iter::repeat(false).take(padding))
            .collect();
        assert_eq!(bits.len(), NUM_BITS + padding);

        bits.chunks_exact(WINDOW_SIZE)
            .map(|x| {
                let mut chunks = [false; WINDOW_SIZE];
                chunks.copy_from_slice(x);
                chunks
            })
            .collect()
    }

    pub fn decompose(
        &self,
        mut region: Region<'_, pallas::Base>,
        z_0: AssignedCell<pallas::Base, pallas::Base>,
        offset: usize,
    ) -> Result<(), plonk::Error> {
        assert!(WINDOW_SIZE * NUM_WINDOWS < NUM_BITS + WINDOW_SIZE);

        // Enable selectors
        for index in 0..NUM_WINDOWS {
            self.config.s_rc.enable(&mut region, index + offset)?;
        }

        let mut z_values: Vec<AssignedCell<pallas::Base, pallas::Base>> = vec![z_0.clone()];
        let mut z = z_0.clone();
        let decomposed_chunks =
            z_0.value().map(|val| Self::decompose_value(val)).transpose_vec(NUM_WINDOWS);

        let two_pow_k_inverse =
            Value::known(pallas::Base::from(1 << WINDOW_SIZE as u64).invert().unwrap());
        for (i, chunk) in decomposed_chunks.iter().enumerate() {
            let z_next = {
                let z_curr = z.value().copied();
                let chunk_value = chunk.map(|c| {
                    pallas::Base::from(c.iter().rev().fold(0, |acc, c| (acc << 1) + *c as u64))
                });
                // z_next = (z_curr - k_i) / 2^K
                let z_next = (z_curr - chunk_value) * two_pow_k_inverse;
                region.assign_advice(
                    || format!("z_{}", i + offset + 1),
                    self.config.z,
                    i + offset,
                    || z_next,
                )?
            };
            z_values.push(z_next.clone());
            z = z_next.clone();
        }

        assert!(z_values.len() == NUM_WINDOWS + 1);
        region.constrain_constant(z_values.last().unwrap().cell(), pallas::Base::zero())?;
        Ok(())
    }

    pub fn witness_range_check(
        &self,
        layouter: &mut impl Layouter<pallas::Base>,
        value: Value<pallas::Base>,
    ) -> Result<(), plonk::Error> {
        layouter.assign_region(
            || format!("witness {}-bit native range check", NUM_BITS),
            |mut region: Region<'_, pallas::Base>| {
                let z_0 = region.assign_advice(|| "z_0", self.config.z, 0, || value)?;
                self.decompose(region, z_0, 0)?;
                Ok(())
            },
        )
    }

    pub fn copy_range_check(
        &self,
        layouter: &mut impl Layouter<pallas::Base>,
        value: AssignedCell<pallas::Base, pallas::Base>,
    ) -> Result<(), plonk::Error> {
        layouter.assign_region(
            || format!("copy {}-bit native range check", NUM_BITS),
            |mut region: Region<'_, pallas::Base>| {
                let z_0 = value.copy_advice(|| "z_0", &mut region, self.config.z, 0)?;
                self.decompose(region, z_0, 0)?;
                Ok(())
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use group::ff::PrimeField;
    use halo2_proofs::{
        circuit::floor_planner,
        dev::{CircuitLayout, MockProver},
        plonk::Circuit,
    };
    use pasta_curves::arithmetic::FieldExt;

    #[derive(Clone)]
    struct Range64CircuitConfig {
        rangecheck_config: NativeRangeCheckConfig<3, 64, 22>,
    }

    impl Range64CircuitConfig {
        fn rangecheck_chip(&self) -> NativeRangeCheckChip<3, 64, 22> {
            NativeRangeCheckChip::<3, 64, 22>::construct(self.rangecheck_config.clone())
        }
    }

    #[derive(Default)]
    struct Range64Circuit {
        a: Value<pallas::Base>,
    }

    impl Circuit<pallas::Base> for Range64Circuit {
        type Config = Range64CircuitConfig;
        type FloorPlanner = floor_planner::V1;

        fn without_witnesses(&self) -> Self {
            Self::default()
        }

        fn configure(meta: &mut ConstraintSystem<pallas::Base>) -> Self::Config {
            let table_column = meta.lookup_table_column();

            let constants = meta.fixed_column();
            meta.enable_constant(constants);

            let z = meta.advice_column();
            meta.enable_equality(z);

            let rangecheck_config =
                NativeRangeCheckChip::<3, 64, 22>::configure(meta, z, table_column);

            Range64CircuitConfig { rangecheck_config }
        }

        fn synthesize(
            &self,
            config: Self::Config,
            mut layouter: impl Layouter<pallas::Base>,
        ) -> Result<(), plonk::Error> {
            let rangecheck_chip = config.rangecheck_chip();

            rangecheck_chip.load_k_table(&mut layouter, config.rangecheck_config.k_values_table)?;
            rangecheck_chip.witness_range_check(&mut layouter, self.a)?;

            Ok(())
        }
    }

    // cargo test --release --all-features --lib native_range_check -- --nocapture
    #[test]
    fn native_range_check_64() {
        let k = 5;

        let valid_values = vec![
            pallas::Base::zero(),
            pallas::Base::one(),
            pallas::Base::from(u64::MAX),
            pallas::Base::from(rand::random::<u64>()),
        ];

        let invalid_values = vec![
            -pallas::Base::one(),
            pallas::Base::from_u128(u64::MAX as u128 + 1),
            -pallas::Base::from_u128(u64::MAX as u128 + 1),
            pallas::Base::from_u128(rand::random::<u128>()),
            // The following two are valid
            // 2 = -28948022309329048855892746252171976963363056481941560715954676764349967630335
            //-pallas::Base::from_str_vartime(
            //    "28948022309329048855892746252171976963363056481941560715954676764349967630335",
            //)
            //.unwrap(),
            // 1 = -28948022309329048855892746252171976963363056481941560715954676764349967630336
            //-pallas::Base::from_str_vartime(
            //    "28948022309329048855892746252171976963363056481941560715954676764349967630336",
            //)
            //.unwrap(),
        ];

        use plotters::prelude::*;
        let circuit = Range64Circuit { a: Value::known(pallas::Base::one()) };
        let root =
            BitMapBackend::new("target/native_range_check_64_circuit_layout.png", (3840, 2160))
                .into_drawing_area();
        root.fill(&WHITE).unwrap();
        let root =
            root.titled("64-bit Native Range Check Circuit Layout", ("sans-serif", 60)).unwrap();
        CircuitLayout::default().render(k, &circuit, &root).unwrap();

        for i in valid_values {
            println!("64-bit (valid) range check for {:?}", i);
            let circuit = Range64Circuit { a: Value::known(i) };
            let prover = MockProver::run(k, &circuit, vec![]).unwrap();
            prover.assert_satisfied();
        }

        for i in invalid_values {
            println!("64-bit (invalid) range check for {:?}", i);
            let circuit = Range64Circuit { a: Value::known(i) };
            let prover = MockProver::run(k, &circuit, vec![]).unwrap();
            assert!(prover.verify().is_err());
        }
    }
}
