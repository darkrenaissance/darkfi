/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
 * Copyright (C) 2022 zkMove Authors (Apache-2.0)
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

use std::marker::PhantomData;

use halo2_proofs::{
    circuit::{AssignedCell, Chip, Layouter, Region},
    pasta::group::ff::WithSmallOrderMulGroup,
    plonk::{self, Advice, Column, ConstraintSystem, Expression, Selector},
    poly::Rotation,
};

const NUM_OF_UTILITY_ADVICE_COLUMNS: usize = 4;

#[derive(Clone, Debug)]
pub struct IsEqualConfig<F: WithSmallOrderMulGroup<3> + Ord> {
    s_is_eq: Selector,
    advices: [Column<Advice>; NUM_OF_UTILITY_ADVICE_COLUMNS],
    _marker: PhantomData<F>,
}

pub struct IsEqualChip<F: WithSmallOrderMulGroup<3> + Ord> {
    config: IsEqualConfig<F>,
}

impl<F: WithSmallOrderMulGroup<3> + Ord> Chip<F> for IsEqualChip<F> {
    type Config = IsEqualConfig<F>;
    type Loaded = ();

    fn config(&self) -> &Self::Config {
        &self.config
    }

    fn loaded(&self) -> &Self::Loaded {
        &()
    }
}

impl<F: WithSmallOrderMulGroup<3> + Ord> IsEqualChip<F> {
    pub fn construct(config: <Self as Chip<F>>::Config) -> Self {
        Self { config }
    }

    pub fn configure(
        meta: &mut ConstraintSystem<F>,
        advices: [Column<Advice>; NUM_OF_UTILITY_ADVICE_COLUMNS],
    ) -> <Self as Chip<F>>::Config {
        let s_is_eq = meta.selector();
        meta.create_gate("is_eq", |meta| {
            let lhs = meta.query_advice(advices[0], Rotation::cur());
            let rhs = meta.query_advice(advices[1], Rotation::cur());
            let out = meta.query_advice(advices[2], Rotation::cur());
            let delta_invert = meta.query_advice(advices[3], Rotation::cur());
            let s_is_eq = meta.query_selector(s_is_eq);
            let one = Expression::Constant(F::ONE);

            vec![
                // out is 0 or 1
                s_is_eq.clone() * (out.clone() * (one.clone() - out.clone())),
                // if a != b then (a - b) * inverse(a - b) == 1 - out
                // if a == b then (a - b) * 1 == 1 - out
                s_is_eq.clone() *
                    ((lhs.clone() - rhs.clone()) * delta_invert.clone() + (out - one.clone())),
                // constrain delta_invert: (a - b) * inverse(a - b) must be 1 or 0
                s_is_eq * (lhs.clone() - rhs.clone()) * ((lhs - rhs) * delta_invert - one),
            ]
        });

        IsEqualConfig { s_is_eq, advices, _marker: PhantomData }
    }

    pub fn is_eq_with_output(
        &self,
        layouter: &mut impl Layouter<F>,
        a: AssignedCell<F, F>,
        b: AssignedCell<F, F>,
    ) -> Result<AssignedCell<F, F>, plonk::Error> {
        let config = self.config();

        let out = layouter.assign_region(
            || "is_eq",
            |mut region: Region<'_, F>| {
                config.s_is_eq.enable(&mut region, 0)?;

                a.copy_advice(|| "copy a", &mut region, config.advices[0], 0)?;
                b.copy_advice(|| "copy b", &mut region, config.advices[1], 0)?;

                let delta_invert = a.value().copied().to_field().zip(b.value()).map(|(a, b)| {
                    if a == b.into() {
                        F::ONE.into()
                    } else {
                        let delta = a - *b;
                        delta.invert()
                    }
                });

                region.assign_advice(|| "delta invert", config.advices[3], 0, || delta_invert)?;

                let is_eq =
                    a.value_field().evaluate().zip(b.value_field().evaluate()).map(|(lhs, rhs)| {
                        if lhs == rhs {
                            F::ONE
                        } else {
                            F::ZERO
                        }
                    });

                let cell = region.assign_advice(|| "is_eq", config.advices[2], 0, || is_eq)?;
                Ok(cell)
            },
        )?;

        Ok(out)
    }
}

#[derive(Clone, Debug)]
pub struct AssertEqualConfig<F: WithSmallOrderMulGroup<3> + Ord> {
    s_eq: Selector,
    advices: [Column<Advice>; 2],
    _marker: PhantomData<F>,
}

pub struct AssertEqualChip<F: WithSmallOrderMulGroup<3> + Ord> {
    config: AssertEqualConfig<F>,
}

impl<F: WithSmallOrderMulGroup<3> + Ord> Chip<F> for AssertEqualChip<F> {
    type Config = AssertEqualConfig<F>;
    type Loaded = ();

    fn config(&self) -> &Self::Config {
        &self.config
    }

    fn loaded(&self) -> &Self::Loaded {
        &()
    }
}

impl<F: WithSmallOrderMulGroup<3> + Ord> AssertEqualChip<F> {
    pub fn construct(config: <Self as Chip<F>>::Config) -> Self {
        Self { config }
    }

    pub fn configure(
        meta: &mut ConstraintSystem<F>,
        advices: [Column<Advice>; 2],
    ) -> <Self as Chip<F>>::Config {
        let s_eq = meta.selector();
        meta.create_gate("assert_eq", |meta| {
            let lhs = meta.query_advice(advices[0], Rotation::cur());
            let rhs = meta.query_advice(advices[1], Rotation::cur());
            let s_eq = meta.query_selector(s_eq);

            vec![s_eq * (lhs - rhs)]
        });

        AssertEqualConfig { s_eq, advices, _marker: PhantomData }
    }

    pub fn assert_equal(
        &self,
        layouter: &mut impl Layouter<F>,
        a: AssignedCell<F, F>,
        b: AssignedCell<F, F>,
    ) -> Result<(), plonk::Error> {
        let config = self.config();

        layouter.assign_region(
            || "assert_eq",
            |mut region: Region<'_, F>| {
                config.s_eq.enable(&mut region, 0)?;

                a.copy_advice(|| "copy a", &mut region, config.advices[0], 0)?;
                b.copy_advice(|| "copy b", &mut region, config.advices[1], 0)?;
                Ok(())
            },
        )?;

        Ok(())
    }
}
