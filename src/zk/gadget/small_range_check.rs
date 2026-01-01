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

use std::marker::PhantomData;

use halo2_proofs::{
    circuit::{AssignedCell, Chip, Layouter},
    pasta::group::ff::WithSmallOrderMulGroup,
    plonk,
    plonk::{Advice, Column, ConstraintSystem, Constraints, Expression, Selector},
    poly::Rotation,
};

/// Checks that an expression is in the small range [0..range),
/// i.e. 0 ≤ word < range.
pub fn range_check<F: WithSmallOrderMulGroup<3> + Ord>(
    word: Expression<F>,
    range: u8,
) -> Expression<F> {
    assert!(range > 0);

    (1..(range as usize))
        .fold(word.clone(), |acc, i| acc * (Expression::Constant(F::from(i as u64)) - word.clone()))
}

#[derive(Clone, Debug)]
pub struct SmallRangeCheckConfig {
    pub z: Column<Advice>,
    pub selector: Selector,
}

#[derive(Clone, Debug)]
pub struct SmallRangeCheckChip<F> {
    config: SmallRangeCheckConfig,
    _marker: PhantomData<F>,
}

impl<F: WithSmallOrderMulGroup<3> + Ord> Chip<F> for SmallRangeCheckChip<F> {
    type Config = SmallRangeCheckConfig;
    type Loaded = ();

    fn config(&self) -> &Self::Config {
        &self.config
    }

    fn loaded(&self) -> &Self::Loaded {
        &()
    }
}

impl<F: WithSmallOrderMulGroup<3> + Ord> SmallRangeCheckChip<F> {
    pub fn construct(config: SmallRangeCheckConfig) -> Self {
        Self { config, _marker: PhantomData }
    }

    pub fn configure(
        meta: &mut ConstraintSystem<F>,
        z: Column<Advice>,
        range: u8,
    ) -> SmallRangeCheckConfig {
        // Enable permutation on z column
        meta.enable_equality(z);

        let selector = meta.selector();

        meta.create_gate("bool check", |meta| {
            let selector = meta.query_selector(selector);
            let advice = meta.query_advice(z, Rotation::cur());
            Constraints::with_selector(selector, Some(range_check(advice, range)))
        });

        SmallRangeCheckConfig { z, selector }
    }

    pub fn small_range_check(
        &self,
        mut layouter: impl Layouter<F>,
        value: AssignedCell<F, F>,
    ) -> Result<(), plonk::Error> {
        layouter.assign_region(
            || "small range constrain",
            |mut region| {
                self.config.selector.enable(&mut region, 0)?;
                value.copy_advice(|| "z_0", &mut region, self.config.z, 0)?;
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
        circuit::{floor_planner, Value},
        dev::MockProver,
        pasta::pallas,
        plonk::Circuit,
    };

    #[derive(Default)]
    struct SmallRangeCircuit {
        value: Value<pallas::Base>,
    }

    impl Circuit<pallas::Base> for SmallRangeCircuit {
        type Config = (SmallRangeCheckConfig, Column<Advice>);
        type FloorPlanner = floor_planner::V1;
        type Params = ();

        fn without_witnesses(&self) -> Self {
            Self::default()
        }

        fn configure(meta: &mut ConstraintSystem<pallas::Base>) -> Self::Config {
            let w = meta.advice_column();
            let z = meta.advice_column();

            meta.enable_equality(w);

            // One bit
            let config = SmallRangeCheckChip::configure(meta, z, 2);

            (config, w)
        }

        fn synthesize(
            &self,
            config: Self::Config,
            mut layouter: impl Layouter<pallas::Base>,
        ) -> Result<(), plonk::Error> {
            let chip = SmallRangeCheckChip::construct(config.0.clone());
            let value = assign_free_advice(layouter.namespace(|| "val"), config.1, self.value)?;
            chip.small_range_check(layouter.namespace(|| "boolean check"), value)?;
            Ok(())
        }
    }

    #[test]
    fn boolean_range_check() {
        let k = 3;

        for i in 0..2 {
            let circuit = SmallRangeCircuit { value: Value::known(pallas::Base::from(i as u64)) };
            let prover = MockProver::run(k, &circuit, vec![]).unwrap();
            prover.assert_satisfied();
        }

        let circuit = SmallRangeCircuit { value: Value::known(pallas::Base::from(2)) };
        let prover = MockProver::run(k, &circuit, vec![]).unwrap();
        assert!(prover.verify().is_err());
    }
}
