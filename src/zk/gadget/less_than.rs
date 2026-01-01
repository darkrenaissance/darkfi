/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

//! Less-Than Gadget
//!
//! Given two values:
//!     - `a`, a NUM_OF_BITS-length value and
//!     - `b`, an arbitrary field element,
//! this gadget constrains them in the following way:
//!     - in `strict` mode, `a` is constrained to be strictly less than `b`;
//!     - else, `a` is constrained to be less than or equal to `b`.

use halo2_proofs::{
    arithmetic::Field,
    circuit::{AssignedCell, Chip, Layouter, Region, Value},
    pasta::pallas,
    plonk::{Advice, Column, ConstraintSystem, Error, Expression, Selector, TableColumn},
    poly::Rotation,
};

use super::native_range_check::{NativeRangeCheckChip, NativeRangeCheckConfig};

#[derive(Clone, Debug)]
pub struct LessThanConfig<const WINDOW_SIZE: usize, const NUM_OF_BITS: usize> {
    pub s_lt: Selector,
    pub s_leq: Selector,
    pub a: Column<Advice>,
    pub b: Column<Advice>,
    pub a_offset: Column<Advice>,
    pub range_a_config: NativeRangeCheckConfig<WINDOW_SIZE, NUM_OF_BITS>,
    pub range_a_offset_config: NativeRangeCheckConfig<WINDOW_SIZE, NUM_OF_BITS>,
    pub k_values_table: TableColumn,
}

#[derive(Clone, Debug)]
pub struct LessThanChip<const WINDOW_SIZE: usize, const NUM_OF_BITS: usize> {
    config: LessThanConfig<WINDOW_SIZE, NUM_OF_BITS>,
}

impl<const WINDOW_SIZE: usize, const NUM_OF_BITS: usize> Chip<pallas::Base>
    for LessThanChip<WINDOW_SIZE, NUM_OF_BITS>
{
    type Config = LessThanConfig<WINDOW_SIZE, NUM_OF_BITS>;
    type Loaded = ();

    fn config(&self) -> &Self::Config {
        &self.config
    }

    fn loaded(&self) -> &Self::Loaded {
        &()
    }
}

impl<const WINDOW_SIZE: usize, const NUM_OF_BITS: usize> LessThanChip<WINDOW_SIZE, NUM_OF_BITS> {
    pub fn construct(config: LessThanConfig<WINDOW_SIZE, NUM_OF_BITS>) -> Self {
        Self { config }
    }

    pub fn configure(
        meta: &mut ConstraintSystem<pallas::Base>,
        a: Column<Advice>,
        b: Column<Advice>,
        a_offset: Column<Advice>,
        z1: Column<Advice>,
        z2: Column<Advice>,
        k_values_table: TableColumn,
    ) -> LessThanConfig<WINDOW_SIZE, NUM_OF_BITS> {
        let s_lt = meta.selector();
        let s_leq = meta.selector();

        meta.enable_equality(a);
        meta.enable_equality(b);
        meta.enable_equality(a_offset);
        meta.enable_equality(z1);
        meta.enable_equality(z2);

        // configure range check for `a` and `offset`
        let range_a_config =
            NativeRangeCheckChip::<WINDOW_SIZE, NUM_OF_BITS>::configure(meta, z1, k_values_table);

        let range_a_offset_config =
            NativeRangeCheckChip::<WINDOW_SIZE, NUM_OF_BITS>::configure(meta, z2, k_values_table);

        let config = LessThanConfig {
            s_lt,
            s_leq,
            a,
            b,
            a_offset,
            range_a_config,
            range_a_offset_config,
            k_values_table,
        };

        meta.create_gate("a_offset", |meta| {
            let s_lt = meta.query_selector(config.s_lt);
            let s_leq = meta.query_selector(config.s_leq);
            let a = meta.query_advice(config.a, Rotation::cur());
            let b = meta.query_advice(config.b, Rotation::cur());
            let a_offset = meta.query_advice(config.a_offset, Rotation::cur());
            let two_pow_m =
                Expression::Constant(pallas::Base::from(2).pow([NUM_OF_BITS as u64, 0, 0, 0]));

            // If strict, a_offset = a + 2^m - b
            let strict_check =
                s_lt * (a_offset.clone() - two_pow_m.clone() + b.clone() - a.clone());
            // If leq, a_offset = a + 2^m - b - 1
            let leq_check =
                s_leq * (a_offset - two_pow_m + b - a + Expression::Constant(pallas::Base::one()));

            vec![strict_check, leq_check]
        });

        config
    }

    pub fn witness_less_than(
        &self,
        mut layouter: impl Layouter<pallas::Base>,
        a: Value<pallas::Base>,
        b: Value<pallas::Base>,
        offset: usize,
        strict: bool,
    ) -> Result<(), Error> {
        let (a, _, a_offset) = layouter.assign_region(
            || "a less than b",
            |mut region: Region<'_, pallas::Base>| {
                let a = region.assign_advice(|| "a", self.config.a, offset, || a)?;
                let b = region.assign_advice(|| "b", self.config.b, offset, || b)?;
                let a_offset = self.less_than(region, a.clone(), b.clone(), offset, strict)?;
                Ok((a, b, a_offset))
            },
        )?;

        self.less_than_range_check(layouter, a, a_offset)?;

        Ok(())
    }

    /*
    pub fn witness_less_than2(
        &self,
        mut layouter: impl Layouter<pallas::Base>,
        a: Value<pallas::Base>,
        b: Value<pallas::Base>,
        offset: usize,
        strict: bool,
    ) -> Result<AssignedCell<pallas::Base, pallas::Base>, Error> {
        let (a, _, a_offset) = layouter.assign_region(
            || "a less than b",
            |mut region: Region<'_, pallas::Base>| {
                let a = region.assign_advice(|| "a", self.config.a, offset, || a)?;
                let b = region.assign_advice(|| "b", self.config.b, offset, || b)?;
                let a_offset = self.less_than(region, a.clone(), b.clone(), offset)?;
                Ok((a, b, a_offset))
            },
        )?;

        self.less_than_range_check(layouter, a, a_offset.clone(), strict)?;

        Ok(a_offset)
    }
    */

    pub fn copy_less_than(
        &self,
        mut layouter: impl Layouter<pallas::Base>,
        a: AssignedCell<pallas::Base, pallas::Base>,
        b: AssignedCell<pallas::Base, pallas::Base>,
        offset: usize,
        strict: bool,
    ) -> Result<(), Error> {
        let (a, _, a_offset) = layouter.assign_region(
            || "a less than b",
            |mut region: Region<'_, pallas::Base>| {
                let a = a.copy_advice(|| "a", &mut region, self.config.a, offset)?;
                let b = b.copy_advice(|| "b", &mut region, self.config.b, offset)?;
                let a_offset = self.less_than(region, a.clone(), b.clone(), offset, strict)?;
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
        let range_a_chip = NativeRangeCheckChip::<WINDOW_SIZE, NUM_OF_BITS>::construct(
            self.config.range_a_config.clone(),
        );
        let range_a_offset_chip = NativeRangeCheckChip::<WINDOW_SIZE, NUM_OF_BITS>::construct(
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
        strict: bool,
    ) -> Result<AssignedCell<pallas::Base, pallas::Base>, Error> {
        if strict {
            // enable `less_than` selector
            self.config.s_lt.enable(&mut region, offset)?;
        } else {
            self.config.s_leq.enable(&mut region, offset)?;
        }

        let two_pow_m = pallas::Base::from(2).pow([NUM_OF_BITS as u64, 0, 0, 0]);
        let a_offset = if strict {
            a.value().zip(b.value()).map(|(a, b)| *a + (two_pow_m - b))
        } else {
            a.value().zip(b.value()).map(|(a, b)| *a + (two_pow_m - b) - pallas::Base::one())
        };
        let a_offset =
            region.assign_advice(|| "a_offset", self.config.a_offset, offset, || a_offset)?;

        Ok(a_offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkfi_sdk::crypto::pasta_prelude::PrimeField;
    use halo2_proofs::{
        circuit::floor_planner,
        dev::{CircuitLayout, MockProver},
        plonk::Circuit,
    };

    macro_rules! test_circuit {
        ($k: expr, $strict:expr, $window_size:expr, $num_bits:expr, $valid_pairs:expr, $invalid_pairs:expr) => {
            #[derive(Default)]
            struct LessThanCircuit {
                a: Value<pallas::Base>,
                b: Value<pallas::Base>,
            }

            impl Circuit<pallas::Base> for LessThanCircuit {
                type Config = (LessThanConfig<$window_size, $num_bits>, Column<Advice>);
                type FloorPlanner = floor_planner::V1;
                type Params = ();

                fn without_witnesses(&self) -> Self {
                    Self { a: Value::unknown(), b: Value::unknown() }
                }

                fn configure(meta: &mut ConstraintSystem<pallas::Base>) -> Self::Config {
                    let w = meta.advice_column();
                    meta.enable_equality(w);

                    let a = meta.advice_column();
                    let b = meta.advice_column();
                    let a_offset = meta.advice_column();
                    let z1 = meta.advice_column();
                    let z2 = meta.advice_column();

                    let k_values_table = meta.lookup_table_column();

                    let constants = meta.fixed_column();
                    meta.enable_constant(constants);

                    (
                        LessThanChip::<$window_size, $num_bits>::configure(
                            meta,
                            a,
                            b,
                            a_offset,
                            z1,
                            z2,
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
                        LessThanChip::<$window_size, $num_bits>::construct(config.0.clone());

                    NativeRangeCheckChip::<$window_size, $num_bits>::load_k_table(
                        &mut layouter,
                        config.0.k_values_table,
                    )?;

                    less_than_chip.witness_less_than(
                        layouter.namespace(|| "a < b"),
                        self.a,
                        self.b,
                        0,
                        $strict,
                    )?;

                    Ok(())
                }
            }

            use plotters::prelude::*;
            let circuit = LessThanCircuit {
                a: Value::known(pallas::Base::zero()),
                b: Value::known(pallas::Base::one()),
            };
            let file_name = format!("target/lessthan_check_{:?}_circuit_layout.png", $num_bits);
            let root = BitMapBackend::new(file_name.as_str(), (3840, 2160)).into_drawing_area();
            CircuitLayout::default().render($k, &circuit, &root).unwrap();

            let check = if $strict { "<" } else { "<=" };
            for (a, b) in $valid_pairs {
                println!("{:?} bit (valid) {:?} {} {:?} check", $num_bits, a, check, b);
                let circuit = LessThanCircuit { a: Value::known(a), b: Value::known(b) };
                let prover = MockProver::run($k, &circuit, vec![]).unwrap();
                prover.assert_satisfied();
            }

            for (a, b) in $invalid_pairs {
                println!("{:?} bit (invalid) {:?} {} {:?} check", $num_bits, a, check, b);
                let circuit = LessThanCircuit { a: Value::known(a), b: Value::known(b) };
                let prover = MockProver::run($k, &circuit, vec![]).unwrap();
                assert!(prover.verify().is_err())
            }
        };
    }

    #[test]
    fn leq_64() {
        let k = 5;
        const WINDOW_SIZE: usize = 3;
        const NUM_OF_BITS: usize = 64;

        let valid_pairs = [
            (pallas::Base::ZERO, pallas::Base::ZERO),
            (pallas::Base::ONE, pallas::Base::ONE),
            (pallas::Base::from(13), pallas::Base::from(15)),
            (pallas::Base::ZERO, pallas::Base::from(u64::MAX)),
            (pallas::Base::ONE, pallas::Base::from(rand::random::<u64>())),
            (pallas::Base::from(u64::MAX), pallas::Base::from(u64::MAX) + pallas::Base::ONE),
            (pallas::Base::from(u64::MAX), pallas::Base::from(u64::MAX)),
        ];

        let invalid_pairs = [
            (pallas::Base::from(14), pallas::Base::from(11)),
            (pallas::Base::from(u64::MAX), pallas::Base::ZERO),
            (pallas::Base::ONE, pallas::Base::ZERO),
        ];
        test_circuit!(k, false, WINDOW_SIZE, NUM_OF_BITS, valid_pairs, invalid_pairs);
    }

    #[test]
    fn less_than_64() {
        let k = 5;
        const WINDOW_SIZE: usize = 3;
        const NUM_OF_BITS: usize = 64;

        let valid_pairs = [
            (pallas::Base::from(13), pallas::Base::from(15)),
            (pallas::Base::ZERO, pallas::Base::from(u64::MAX)),
            (pallas::Base::ONE, pallas::Base::from(rand::random::<u64>())),
            (pallas::Base::from(u64::MAX), pallas::Base::from(u64::MAX) + pallas::Base::ONE),
        ];

        let invalid_pairs = [
            (pallas::Base::from(14), pallas::Base::from(11)),
            (pallas::Base::from(u64::MAX), pallas::Base::ZERO),
            (pallas::Base::ZERO, pallas::Base::ZERO),
            (pallas::Base::ONE, pallas::Base::ONE),
            (pallas::Base::ONE, pallas::Base::ZERO),
            (pallas::Base::from(u64::MAX), pallas::Base::from(u64::MAX)),
        ];
        test_circuit!(k, true, WINDOW_SIZE, NUM_OF_BITS, valid_pairs, invalid_pairs);
    }

    #[test]
    fn leq_253() {
        let k = 7;
        const WINDOW_SIZE: usize = 3;
        const NUM_OF_BITS: usize = 253;

        const P_MINUS_1: pallas::Base = pallas::Base::from_raw([
            0x992d30ed00000000,
            0x224698fc094cf91b,
            0x0000000000000000,
            0x4000000000000000,
        ]);

        // 2^253 - 1. This is the maximum we can check.
        const MAX_253: pallas::Base = pallas::Base::from_raw([
            0xFFFFFFFFFFFFFFFF,
            0xFFFFFFFFFFFFFFFF,
            0xFFFFFFFFFFFFFFFF,
            0x1FFFFFFFFFFFFFFF,
        ]);

        let valid_pairs = [
            (pallas::Base::ZERO, pallas::Base::ZERO),
            (pallas::Base::ZERO, pallas::Base::ONE),
            (pallas::Base::from(u64::MAX), pallas::Base::from(u64::MAX) + pallas::Base::ONE),
            (
                pallas::Base::from_u128(u128::MAX),
                pallas::Base::from_u128(u128::MAX) + pallas::Base::ONE,
            ),
            (MAX_253, MAX_253),
            (MAX_253 - pallas::Base::from(2), MAX_253 - pallas::Base::ONE),
            (MAX_253 - pallas::Base::ONE, MAX_253),
            (MAX_253, MAX_253 + pallas::Base::ONE),
        ];

        let invalid_pairs = [
            (pallas::Base::ONE, pallas::Base::ZERO),
            (P_MINUS_1 - pallas::Base::ONE, P_MINUS_1),
            (P_MINUS_1, pallas::Base::ZERO),
            (P_MINUS_1, P_MINUS_1),
            (MAX_253, pallas::Base::ZERO),
            (MAX_253, pallas::Base::ONE),
            (MAX_253 + pallas::Base::ONE, pallas::Base::ZERO),
            (MAX_253 + pallas::Base::ONE, pallas::Base::ONE),
            (MAX_253 + pallas::Base::ONE, MAX_253 + pallas::Base::ONE),
            (MAX_253 + pallas::Base::ONE, MAX_253 + pallas::Base::from(2)),
        ];

        test_circuit!(k, false, WINDOW_SIZE, NUM_OF_BITS, valid_pairs, invalid_pairs);
    }

    #[test]
    fn less_than_253() {
        let k = 7;
        const WINDOW_SIZE: usize = 3;
        const NUM_OF_BITS: usize = 253;

        const P_MINUS_1: pallas::Base = pallas::Base::from_raw([
            0x992d30ed00000000,
            0x224698fc094cf91b,
            0x0000000000000000,
            0x4000000000000000,
        ]);

        // 2^253 - 1. This is the maximum we can check.
        const MAX_253: pallas::Base = pallas::Base::from_raw([
            0xFFFFFFFFFFFFFFFF,
            0xFFFFFFFFFFFFFFFF,
            0xFFFFFFFFFFFFFFFF,
            0x1FFFFFFFFFFFFFFF,
        ]);

        let valid_pairs = [
            (pallas::Base::ZERO, pallas::Base::ONE),
            (pallas::Base::from(u64::MAX), pallas::Base::from(u64::MAX) + pallas::Base::ONE),
            (
                pallas::Base::from_u128(u128::MAX),
                pallas::Base::from_u128(u128::MAX) + pallas::Base::ONE,
            ),
            (MAX_253 - pallas::Base::from(2), MAX_253 - pallas::Base::ONE),
            (MAX_253 - pallas::Base::ONE, MAX_253),
            (MAX_253, MAX_253 + pallas::Base::ONE),
        ];

        let invalid_pairs = [
            (pallas::Base::ZERO, pallas::Base::ZERO),
            (pallas::Base::ONE, pallas::Base::ZERO),
            (P_MINUS_1 - pallas::Base::ONE, P_MINUS_1),
            (P_MINUS_1, P_MINUS_1),
            (P_MINUS_1, pallas::Base::ZERO),
            (MAX_253, MAX_253),
            (MAX_253, pallas::Base::ZERO),
            (MAX_253, pallas::Base::ONE),
            (MAX_253 + pallas::Base::ONE, pallas::Base::ZERO),
            (MAX_253 + pallas::Base::ONE, pallas::Base::ONE),
            (MAX_253 + pallas::Base::ONE, MAX_253 + pallas::Base::ONE),
            (MAX_253 + pallas::Base::ONE, MAX_253 + pallas::Base::from(2)),
        ];

        test_circuit!(k, true, WINDOW_SIZE, NUM_OF_BITS, valid_pairs, invalid_pairs);
    }
}
