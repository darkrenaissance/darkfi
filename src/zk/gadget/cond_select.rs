/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
 *
 * Copyright (c) zkMove Authors
 * SPDX-License-Identifier: Apache-2.0
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

use core::marker::PhantomData;

use halo2_proofs::{
    circuit::{AssignedCell, Chip, Layouter, Region},
    pasta::group::ff::WithSmallOrderMulGroup,
    plonk::{Advice, Column, ConstraintSystem, Error, Expression, Selector},
    poly::Rotation,
};

pub const NUM_OF_UTILITY_ADVICE_COLUMNS: usize = 4;

#[derive(Clone, Debug)]
pub struct ConditionalSelectConfig<F: WithSmallOrderMulGroup<3> + Ord> {
    advices: [Column<Advice>; NUM_OF_UTILITY_ADVICE_COLUMNS],
    s_cs: Selector,
    _marker: PhantomData<F>,
}

pub struct ConditionalSelectChip<F: WithSmallOrderMulGroup<3> + Ord> {
    config: ConditionalSelectConfig<F>,
}

impl<F: WithSmallOrderMulGroup<3> + Ord> Chip<F> for ConditionalSelectChip<F> {
    type Config = ConditionalSelectConfig<F>;
    type Loaded = ();

    fn config(&self) -> &Self::Config {
        &self.config
    }

    fn loaded(&self) -> &Self::Loaded {
        &()
    }
}

impl<F: WithSmallOrderMulGroup<3> + Ord> ConditionalSelectChip<F> {
    pub fn construct(config: <Self as Chip<F>>::Config) -> Self {
        Self { config }
    }

    pub fn configure(
        meta: &mut ConstraintSystem<F>,
        advices: [Column<Advice>; NUM_OF_UTILITY_ADVICE_COLUMNS],
    ) -> <Self as Chip<F>>::Config {
        for column in &advices {
            meta.enable_equality(*column);
        }
        let s_cs = meta.selector();

        meta.create_gate("conditional_select", |meta| {
            let lhs = meta.query_advice(advices[0], Rotation::cur());
            let rhs = meta.query_advice(advices[1], Rotation::cur());
            let out = meta.query_advice(advices[2], Rotation::cur());
            let cond = meta.query_advice(advices[3], Rotation::cur());
            let s_cs = meta.query_selector(s_cs);
            let one = Expression::Constant(F::ONE);

            vec![
                // cond is 0 or 1
                s_cs.clone() * (cond.clone() * (one - cond.clone())),
                // lhs * cond + rhs * (1 - cond) = out
                s_cs * ((lhs - rhs.clone()) * cond + rhs - out),
            ]
        });

        ConditionalSelectConfig { advices, s_cs, _marker: PhantomData }
    }

    pub fn conditional_select(
        &self,
        layouter: &mut impl Layouter<F>,
        a: AssignedCell<F, F>,
        b: AssignedCell<F, F>,
        cond: AssignedCell<F, F>,
    ) -> Result<AssignedCell<F, F>, Error> {
        let config = self.config();
        let out = layouter.assign_region(
            || "conditional_select",
            |mut region: Region<'_, F>| {
                config.s_cs.enable(&mut region, 0)?;

                a.copy_advice(|| "copy a", &mut region, config.advices[0], 0)?;
                b.copy_advice(|| "copy b", &mut region, config.advices[1], 0)?;

                let cond = cond.copy_advice(|| "copy cond", &mut region, config.advices[3], 0)?;

                let selected = cond
                    .value()
                    .copied()
                    .to_field()
                    .zip(a.value())
                    .zip(b.value())
                    .map(|((cond, a), b)| if cond == F::ONE.into() { a } else { b })
                    .copied();

                let cell =
                    region.assign_advice(|| "select result", config.advices[2], 0, || selected)?;

                Ok(cell)
            },
        )?;
        Ok(out)
    }
}
