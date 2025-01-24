/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use halo2_proofs::{
    circuit::{AssignedCell, Chip, Layouter, Region, Value},
    pasta::{
        group::ff::{Field, PrimeFieldBits},
        pallas,
    },
    plonk,
    plonk::{Advice, Column, ConstraintSystem, Constraints, Selector, TableColumn},
    poly::Rotation,
};

#[derive(Clone, Debug)]
pub struct NativeRangeCheckConfig<const WINDOW_SIZE: usize, const NUM_BITS: usize> {
    pub z: Column<Advice>,
    pub s_rc: Selector,
    pub s_short: Selector,
    pub k_values_table: TableColumn,
}

#[derive(Clone, Debug)]
pub struct NativeRangeCheckChip<const WINDOW_SIZE: usize, const NUM_BITS: usize> {
    config: NativeRangeCheckConfig<WINDOW_SIZE, NUM_BITS>,
}

impl<const WINDOW_SIZE: usize, const NUM_BITS: usize> Chip<pallas::Base>
    for NativeRangeCheckChip<WINDOW_SIZE, NUM_BITS>
{
    type Config = NativeRangeCheckConfig<WINDOW_SIZE, NUM_BITS>;
    type Loaded = ();

    fn config(&self) -> &Self::Config {
        &self.config
    }

    fn loaded(&self) -> &Self::Loaded {
        &()
    }
}

impl<const WINDOW_SIZE: usize, const NUM_BITS: usize> NativeRangeCheckChip<WINDOW_SIZE, NUM_BITS> {
    pub fn construct(config: NativeRangeCheckConfig<WINDOW_SIZE, NUM_BITS>) -> Self {
        Self { config }
    }

    pub fn configure(
        meta: &mut ConstraintSystem<pallas::Base>,
        z: Column<Advice>,
        k_values_table: TableColumn,
    ) -> NativeRangeCheckConfig<WINDOW_SIZE, NUM_BITS> {
        // Enable permutation on z column
        meta.enable_equality(z);

        let s_rc = meta.complex_selector();
        let s_short = meta.complex_selector();

        // Running sum decomposition
        meta.lookup(|meta| {
            let s_rc = meta.query_selector(s_rc);
            let z_curr = meta.query_advice(z, Rotation::cur());
            let z_next = meta.query_advice(z, Rotation::next());

            //    z_next = (z_curr - k_i) / 2^K
            // => k_i = z_curr - (z_next * 2^K)
            vec![(s_rc * (z_curr - z_next * pallas::Base::from(1 << WINDOW_SIZE)), k_values_table)]
        });

        // Checks that are enabled if the last chunk is an `s`-bit value
        // where `s < WINDOW_SIZE`:
        //
        //  |s_rc | s_short |                z                |
        //  ---------------------------------------------------
        //  |  1  |    0    |            last_chunk           |
        //  |  0  |    1    |                0                |
        //  |  0  |    0    | last_chunk << (WINDOW_SIZE - s) |

        // Check that `shifted_last_chunk` is `WINDOW_SIZE` bits,
        // where shifted_last_chunk = last_chunk << (WINDOW_SIZE - s)
        //                          = last_chunk * 2^(WINDOW_SIZE - s)
        meta.lookup(|meta| {
            let s_short = meta.query_selector(s_short);
            let shifted_last_chunk = meta.query_advice(z, Rotation::next());
            vec![(s_short * shifted_last_chunk, k_values_table)]
        });

        // Check that `shifted_last_chunk = last_chunk << (WINDOW_SIZE - s)`
        meta.create_gate("Short lookup bitshift", |meta| {
            let two_pow_window_size = pallas::Base::from(1 << WINDOW_SIZE);
            let s_short = meta.query_selector(s_short);
            let last_chunk = meta.query_advice(z, Rotation::prev());
            // Rotation::cur() is copy-constrained to be zero elsewhere in this gadget.
            let shifted_last_chunk = meta.query_advice(z, Rotation::next());
            // inv_two_pow_s = 1 >> s = 2^{-s}
            let inv_two_pow_s = {
                let s = NUM_BITS % WINDOW_SIZE;
                pallas::Base::from(1 << s).invert().unwrap()
            };

            // shifted_last_chunk = last_chunk << (WINDOW_SIZE - s)
            //                    = last_chunk * 2^WINDOW_SIZE * 2^{-s}
            Constraints::with_selector(
                s_short,
                Some(last_chunk * two_pow_window_size * inv_two_pow_s - shifted_last_chunk),
            )
        });

        NativeRangeCheckConfig { z, s_rc, s_short, k_values_table }
    }

    /// `k_values_table` should be reused across different chips
    /// which is why we don't limit it to a specific instance.
    pub fn load_k_table(
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
        let bits: Vec<_> = value
            .to_le_bits()
            .into_iter()
            .take(NUM_BITS)
            .chain(std::iter::repeat(false).take(WINDOW_SIZE - (NUM_BITS % WINDOW_SIZE)))
            .collect();

        bits.chunks_exact(WINDOW_SIZE)
            .map(|x| {
                // Because bits <= WINDOW_SIZE * NUM_BITS, the last window may be
                // smaller than WINDOW_SIZE.
                // Additionally we have a slice, so convert them all to a fixed length array.
                let mut chunks = [false; WINDOW_SIZE];
                chunks.copy_from_slice(x);
                chunks
            })
            .collect()
    }

    /// This is the main chip function. Attempts to witness the bits for `z_0` proving
    /// it is within the allowed range.
    pub fn decompose(
        &self,
        region: &mut Region<'_, pallas::Base>,
        z_0: AssignedCell<pallas::Base, pallas::Base>,
        offset: usize,
    ) -> Result<(), plonk::Error> {
        let num_windows = NUM_BITS.div_ceil(WINDOW_SIZE);

        // The number of bits in the last chunk.
        let last_chunk_length = NUM_BITS - (WINDOW_SIZE * (num_windows - 1));
        assert!(last_chunk_length > 0);

        // Enable selectors for running sum decomposition
        for index in 0..num_windows {
            self.config.s_rc.enable(region, index + offset)?;
        }

        let mut z_values: Vec<AssignedCell<pallas::Base, pallas::Base>> = vec![z_0.clone()];
        let mut z = z_0;
        // Convert `z_0` into a `Vec<Value<Fp>>` where each value corresponds to a chunk.
        let decomposed_chunks = z.value().map(Self::decompose_value).transpose_vec(num_windows);

        let two_pow_k = pallas::Base::from(1 << WINDOW_SIZE as u64);
        let two_pow_k_inverse = Value::known(two_pow_k.invert().unwrap());

        //   z = 2⁰b₀ + 2¹b₁ + ⋯ + 2ⁿbₙ
        //     = c₀ + 2ʷc₁ + 2²ʷc₂ + ⋯ + 2ᵐʷcₘ
        // where cᵢ are the chunks.
        //
        // We want to show each cᵢ consists of WINDOW_SIZE bits which we do using
        // the lookup table.
        // The algo starts with z₀ = z, then calculates:
        //   zᵢ = (zᵢ₋₁ - cᵢ₋₁)/2ʷ
        // Doing this for all chunks, we end up with zₘ = 0 which is done after.

        // Loop over the decomposed chunks...
        for (i, chunk) in decomposed_chunks.iter().enumerate() {
            let z_next = {
                let z_curr = z.value().copied();
                // Convert the chunk Value<[bool; WINDOW_SIZE]> into Value<pallas::Base>
                let chunk_value = chunk.map(|c| {
                    pallas::Base::from(c.iter().rev().fold(0, |acc, c| (acc << 1) + *c as u64))
                });
                // Calc z_next = (z_curr - k_i) / 2^K
                let z_next = (z_curr - chunk_value) * two_pow_k_inverse;
                // Witness z_next into the running sum decomposition gate
                region.assign_advice(
                    || format!("z_{}", i + offset + 1),
                    self.config.z,
                    i + offset + 1,
                    || z_next,
                )?
            };
            z_values.push(z_next.clone());
            z = z_next.clone();
        }

        assert!(z_values.len() == num_windows + 1);

        // Constrain the last chunk zₘ = 0
        region.constrain_constant(z_values.last().unwrap().cell(), pallas::Base::zero())?;

        // If the last chunk is `s` bits where `s < WINDOW_SIZE`,
        // perform short range check
        //
        //  |s_rc | s_short |                z                |
        //  ---------------------------------------------------
        //  |  1  |    0    |            last_chunk           |
        //  |  0  |    1    |                0                |
        //  |  0  |    0    | last_chunk << (WINDOW_SIZE - s) |
        //  |  0  |    0    |             1 >> s              |

        if last_chunk_length < WINDOW_SIZE {
            let s_short_offset = num_windows + offset;
            self.config.s_short.enable(region, s_short_offset)?;

            // 1 >> s = 2^{-s}
            let inv_two_pow_s = pallas::Base::from(1 << last_chunk_length).invert().unwrap();
            region.assign_advice_from_constant(
                || "inv_two_pow_s",
                self.config.z,
                s_short_offset + 2,
                inv_two_pow_s,
            )?;

            // shifted_last_chunk = last_chunk * 2^{WINDOW_SIZE-s}
            //                    = last_chunk * 2^WINDOW_SIZE * inv_two_pow_s
            let last_chunk = {
                let chunk = decomposed_chunks.last().unwrap();
                chunk.map(|c| {
                    pallas::Base::from(c.iter().rev().fold(0, |acc, c| (acc << 1) + *c as u64))
                })
            };
            let shifted_last_chunk =
                last_chunk * Value::known(two_pow_k) * Value::known(inv_two_pow_s);
            region.assign_advice(
                || "shifted_last_chunk",
                self.config.z,
                s_short_offset + 1,
                || shifted_last_chunk,
            )?;
        }

        Ok(())
    }

    pub fn witness_range_check(
        &self,
        mut layouter: impl Layouter<pallas::Base>,
        value: Value<pallas::Base>,
    ) -> Result<(), plonk::Error> {
        layouter.assign_region(
            || format!("witness {}-bit native range check", NUM_BITS),
            |mut region: Region<'_, pallas::Base>| {
                let z_0 = region.assign_advice(|| "z_0", self.config.z, 0, || value)?;
                self.decompose(&mut region, z_0, 0)?;
                Ok(())
            },
        )
    }

    pub fn copy_range_check(
        &self,
        mut layouter: impl Layouter<pallas::Base>,
        value: AssignedCell<pallas::Base, pallas::Base>,
    ) -> Result<(), plonk::Error> {
        layouter.assign_region(
            || format!("copy {}-bit native range check", NUM_BITS),
            |mut region: Region<'_, pallas::Base>| {
                let z_0 = value.copy_advice(|| "z_0", &mut region, self.config.z, 0)?;
                self.decompose(&mut region, z_0, 0)?;
                Ok(())
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zk::assign_free_advice;
    use halo2_proofs::{
        circuit::floor_planner,
        dev::{CircuitLayout, MockProver},
        pasta::group::ff::PrimeField,
        plonk::Circuit,
    };

    macro_rules! test_circuit {
        ($k: expr, $window_size:expr, $num_bits: expr, $valid_values:expr, $invalid_values:expr) => {
            #[derive(Default)]
            struct RangeCheckCircuit {
                a: Value<pallas::Base>,
            }

            impl Circuit<pallas::Base> for RangeCheckCircuit {
                type Config = (NativeRangeCheckConfig<$window_size, $num_bits>, Column<Advice>);
                type FloorPlanner = floor_planner::V1;
                type Params = ();

                fn without_witnesses(&self) -> Self {
                    Self::default()
                }

                fn configure(meta: &mut ConstraintSystem<pallas::Base>) -> Self::Config {
                    let w = meta.advice_column();
                    meta.enable_equality(w);
                    let z = meta.advice_column();
                    let table_column = meta.lookup_table_column();

                    let constants = meta.fixed_column();
                    meta.enable_constant(constants);
                    (
                        NativeRangeCheckChip::<$window_size, $num_bits>::configure(
                            meta,
                            z,
                            table_column,
                        ),
                        w,
                    )
                }

                fn synthesize(
                    &self,
                    config: Self::Config,
                    mut layouter: impl Layouter<pallas::Base>,
                ) -> Result<(), plonk::Error> {
                    let rangecheck_chip =
                        NativeRangeCheckChip::<$window_size, $num_bits>::construct(
                            config.0.clone(),
                        );
                    NativeRangeCheckChip::<$window_size, $num_bits>::load_k_table(
                        &mut layouter,
                        config.0.k_values_table,
                    )?;

                    let a = assign_free_advice(layouter.namespace(|| "load a"), config.1, self.a)?;
                    rangecheck_chip
                        .copy_range_check(layouter.namespace(|| "copy a and range check"), a)?;

                    rangecheck_chip.witness_range_check(
                        layouter.namespace(|| "witness a and range check"),
                        self.a,
                    )?;

                    Ok(())
                }
            }

            use plotters::prelude::*;
            let circuit = RangeCheckCircuit { a: Value::known(pallas::Base::one()) };
            let file_name = format!("target/native_range_check_{:?}_circuit_layout.png", $num_bits);
            let root = BitMapBackend::new(file_name.as_str(), (3840, 2160)).into_drawing_area();
            root.fill(&WHITE).unwrap();
            let root = root
                .titled(
                    format!("{:?}-bit Native Range Check Circuit Layout", $num_bits).as_str(),
                    ("sans-serif", 60),
                )
                .unwrap();
            CircuitLayout::default().render($k, &circuit, &root).unwrap();

            for i in $valid_values {
                println!("{:?}-bit (valid) range check for {:?}", $num_bits, i);
                let circuit = RangeCheckCircuit { a: Value::known(i) };
                let prover = MockProver::run($k, &circuit, vec![]).unwrap();
                prover.assert_satisfied();
                println!("Constraints satisfied");
            }

            for i in $invalid_values {
                println!("{:?}-bit (invalid) range check for {:?}", $num_bits, i);
                let circuit = RangeCheckCircuit { a: Value::known(i) };
                let prover = MockProver::run($k, &circuit, vec![]).unwrap();
                assert!(prover.verify().is_err());
            }
        };
    }

    // cargo test --release --all-features --lib native_range_check -- --nocapture
    #[test]
    fn native_range_check_2() {
        let k = 6;
        const WINDOW_SIZE: usize = 5;
        const NUM_BITS: usize = 2;

        // [0, 1, 2, 3]
        let valid_values: Vec<_> = (0..(1 << NUM_BITS)).map(pallas::Base::from).collect();
        // [4, 5, 6, ..., 32]
        let invalid_values: Vec<_> =
            ((1 << NUM_BITS)..=(1 << WINDOW_SIZE)).map(pallas::Base::from).collect();
        test_circuit!(k, WINDOW_SIZE, NUM_BITS, valid_values, invalid_values);
    }

    #[test]
    fn native_range_check_64() {
        let k = 6;
        const WINDOW_SIZE: usize = 3;
        const NUM_BITS: usize = 64;

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
        test_circuit!(k, WINDOW_SIZE, NUM_BITS, valid_values, invalid_values);
    }

    #[test]
    fn native_range_check_128() {
        let k = 7;
        const WINDOW_SIZE: usize = 3;
        const NUM_BITS: usize = 128;

        let valid_values = vec![
            pallas::Base::zero(),
            pallas::Base::one(),
            pallas::Base::from_u128(u128::MAX),
            pallas::Base::from_u128(rand::random::<u128>()),
        ];

        let invalid_values = vec![
            -pallas::Base::one(),
            pallas::Base::from_u128(u128::MAX) + pallas::Base::one(),
            -pallas::Base::from_u128(u128::MAX) + pallas::Base::one(),
            -pallas::Base::from_u128(u128::MAX),
        ];
        test_circuit!(k, WINDOW_SIZE, NUM_BITS, valid_values, invalid_values);
    }

    #[test]
    fn native_range_check_253() {
        let k = 8;
        const WINDOW_SIZE: usize = 3;
        const NUM_BITS: usize = 253;

        // 2^253 - 1
        let max_253 = pallas::Base::from_str_vartime(
            "14474011154664524427946373126085988481658748083205070504932198000989141204991",
        )
        .unwrap();

        let valid_values = vec![
            pallas::Base::zero(),
            pallas::Base::one(),
            max_253,
            // 2^253 / 2
            pallas::Base::from_str_vartime(
                "7237005577332262213973186563042994240829374041602535252466099000494570602496",
            )
            .unwrap(),
        ];

        let invalid_values = vec![
            -pallas::Base::one(),
            // p - 1
            pallas::Base::from_str_vartime(
                "28948022309329048855892746252171976963363056481941560715954676764349967630336",
            )
            .unwrap(),
            max_253 + pallas::Base::one(),
        ];
        test_circuit!(k, WINDOW_SIZE, NUM_BITS, valid_values, invalid_values);
    }
}
