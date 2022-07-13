use std::marker::PhantomData;

use group::ff::PrimeFieldBits;
use halo2_proofs::{
    arithmetic::FieldExt,
    circuit::{AssignedCell, Chip, Layouter, Region, Value},
    plonk::{Advice, Column, ConstraintSystem, Error, Selector, TableColumn},
    poly::Rotation,
};

#[derive(Clone, Debug)]
pub struct RangeCheckConfig {
    pub z: Column<Advice>,
    pub s_rc: Selector,
    pub k_values_table: TableColumn,
}

pub struct RangeCheckChip<F: FieldExt + PrimeFieldBits, const WINDOW_SIZE: usize> {
    config: RangeCheckConfig,
    _marker: PhantomData<F>,
}

impl<F: FieldExt + PrimeFieldBits, const WINDOW_SIZE: usize> Chip<F>
    for RangeCheckChip<F, WINDOW_SIZE>
{
    type Config = RangeCheckConfig;
    type Loaded = ();

    fn config(&self) -> &Self::Config {
        &self.config
    }

    fn loaded(&self) -> &Self::Loaded {
        &()
    }
}

impl<F: FieldExt + PrimeFieldBits, const WINDOW_SIZE: usize> RangeCheckChip<F, WINDOW_SIZE> {
    pub fn configure(
        meta: &mut ConstraintSystem<F>,
        k_values_table: TableColumn,
    ) -> <Self as Chip<F>>::Config {
        let z = meta.advice_column();
        let s_rc = meta.complex_selector();

        let config = RangeCheckConfig { z, s_rc, k_values_table };

        meta.lookup(|meta| {
            let s_rc = meta.query_selector(config.s_rc);
            let z_curr = meta.query_advice(config.z, Rotation::cur());
            let z_next = meta.query_advice(config.z, Rotation::next());

            //    z_next = (z_curr - k_i) / 2^K
            // => k_i = z_curr - (z_next * 2^K)
            vec![(s_rc * (z_curr - z_next * F::from(1 << WINDOW_SIZE)), config.k_values_table)]
        });

        config
    }

    /// `k_values_table` should be reused across different chips
    /// which is why we don't limit it to a specific instance.
    pub fn load_k_table(
        layouter: &mut impl Layouter<F>,
        k_values_table: TableColumn,
    ) -> Result<(), Error> {
        layouter.assign_table(
            || format!("{} window table", WINDOW_SIZE),
            |mut table| {
                for index in 0..(1 << WINDOW_SIZE) {
                    table.assign_cell(
                        || "table",
                        k_values_table,
                        index,
                        || Value::known(F::from(index as u64)),
                    )?;
                }
                Ok(())
            },
        )
    }

    pub fn range_check(
        &self,
        mut layouter: impl Layouter<F>,
        element: AssignedCell<F, F>,
        num_of_bits: usize,
        num_of_windows: usize,
    ) -> Result<(), Error> {
        assert!(WINDOW_SIZE * num_of_windows < num_of_bits + WINDOW_SIZE);

        layouter.assign_region(
            || "name",
            |mut region: Region<'_, F>| {
                // enable selectors
                for index in 0..num_of_windows {
                    self.config.s_rc.enable(&mut region, index);
                }

                // copy element to z_0
                let z_0 = element.copy_advice(|| "z_0", &mut region, self.config.z, 0)?;

                let mut z_values: Vec<AssignedCell<F, F>> = vec![z_0.clone()];
                let mut z = z_0.clone();
                let decomposed_chunks = z_0
                    .value()
                    .map(|val| decompose_value::<F, WINDOW_SIZE>(val, num_of_bits))
                    .transpose_vec(num_of_windows);

                let two_pow_k_inverse =
                    Value::known(F::from(1 << WINDOW_SIZE as u64).invert().unwrap());
                for (i, chunk) in decomposed_chunks.iter().enumerate() {
                    let z_next = {
                        let z_curr = z.value().copied();
                        let chunk_value = chunk
                            .map(|c| F::from(c.iter().fold(0, |acc, c| (acc << 1) + *c as u64)));
                        // z_next = (z_curr - k_i) / 2^K
                        let z_next = (z_curr - chunk_value) * two_pow_k_inverse;
                        region.assign_advice(
                            || format!("z_{}", i + 1),
                            self.config.z,
                            i + 1,
                            || z_next,
                        )?
                    };
                    z_values.push(z_next.clone());
                    z = z_next.clone();
                }

                assert!(z_values.len() == num_of_windows + 1);

                region.constrain_constant(z_values.last().unwrap().cell(), F::zero())?;

                Ok(())
            },
        );

        Ok(())
    }
}

/// ### Reference  
pub fn decompose_value<F: FieldExt + PrimeFieldBits, const WINDOW_SIZE: usize>(
    value: &F,
    num_of_bits: usize,
) -> Vec<[bool; WINDOW_SIZE]> {
    let padding = (WINDOW_SIZE - num_of_bits % WINDOW_SIZE) % WINDOW_SIZE;

    let bits: Vec<bool> = value
        .to_le_bits()
        .into_iter()
        .take(num_of_bits)
        .chain(std::iter::repeat(false).take(padding))
        .collect();
    assert_eq!(bits.len(), num_of_bits + padding);

    bits.chunks_exact(WINDOW_SIZE)
        .map(|x| {
            let mut chunks = [false; WINDOW_SIZE];
            chunks.copy_from_slice(x);
            chunks
        })
        .collect()
}
