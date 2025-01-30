/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zk::assign_free_advice;

    use halo2_proofs::{
        arithmetic::Field,
        circuit::{floor_planner, Value},
        dev::MockProver,
        pasta::pallas,
        plonk,
        plonk::{Circuit, Instance},
    };
    use rand::rngs::OsRng;

    #[derive(Clone)]
    struct CondSelectConfig {
        primary: Column<Instance>,
        condselect_config: ConditionalSelectConfig<pallas::Base>,
    }

    #[derive(Default)]
    struct CondSelectCircuit {
        pub cond: Value<pallas::Base>,
        pub a: Value<pallas::Base>,
        pub b: Value<pallas::Base>,
    }

    impl Circuit<pallas::Base> for CondSelectCircuit {
        type Config = CondSelectConfig;
        type FloorPlanner = floor_planner::V1;
        type Params = ();

        fn without_witnesses(&self) -> Self {
            Self::default()
        }

        fn configure(meta: &mut ConstraintSystem<pallas::Base>) -> Self::Config {
            let advices: [Column<Advice>; NUM_OF_UTILITY_ADVICE_COLUMNS] = [
                meta.advice_column(),
                meta.advice_column(),
                meta.advice_column(),
                meta.advice_column(),
            ];

            let primary = meta.instance_column();
            meta.enable_equality(primary);

            for advice in advices.iter() {
                meta.enable_equality(*advice);
            }

            let condselect_config = ConditionalSelectChip::configure(meta, advices);

            Self::Config { primary, condselect_config }
        }

        fn synthesize(
            &self,
            config: Self::Config,
            mut layouter: impl Layouter<pallas::Base>,
        ) -> Result<(), plonk::Error> {
            let condselect_chip =
                ConditionalSelectChip::construct(config.condselect_config.clone());

            let cond = assign_free_advice(
                layouter.namespace(|| "Witness cond"),
                config.condselect_config.advices[0],
                self.cond,
            )?;

            let a = assign_free_advice(
                layouter.namespace(|| "Witness a"),
                config.condselect_config.advices[1],
                self.a,
            )?;

            let b = assign_free_advice(
                layouter.namespace(|| "Witness b"),
                config.condselect_config.advices[2],
                self.b,
            )?;

            let selection = condselect_chip.conditional_select(&mut layouter, a, b, cond)?;
            layouter.constrain_instance(selection.cell(), config.primary, 0)?;

            Ok(())
        }
    }

    #[test]
    fn cond_select_chip() -> crate::Result<()> {
        // 1 should select A
        let cond = pallas::Base::ONE;
        let a = pallas::Base::random(&mut OsRng);
        let b = pallas::Base::random(&mut OsRng);
        let public_inputs = vec![a];

        let circuit =
            CondSelectCircuit { cond: Value::known(cond), a: Value::known(a), b: Value::known(b) };

        let prover = MockProver::run(4, &circuit, vec![public_inputs])?;
        prover.assert_satisfied();

        // 0 should select B
        let cond = pallas::Base::ZERO;
        let a = pallas::Base::random(&mut OsRng);
        let b = pallas::Base::random(&mut OsRng);
        let public_inputs = vec![b];

        let circuit =
            CondSelectCircuit { cond: Value::known(cond), a: Value::known(a), b: Value::known(b) };

        let prover = MockProver::run(4, &circuit, vec![public_inputs])?;
        prover.assert_satisfied();

        Ok(())
    }
}
