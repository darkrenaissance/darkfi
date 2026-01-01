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
    plonk::{Advice, Column, ConstraintSystem, Constraints, Selector},
    poly::Rotation,
};

/// Arithmetic instructions implemented in the chip
pub trait ArithInstruction<F: WithSmallOrderMulGroup<3> + Ord>: Chip<F> {
    /// Add two field elements and return their sum
    fn add(
        &self,
        layouter: impl Layouter<F>,
        a: &AssignedCell<F, F>,
        b: &AssignedCell<F, F>,
    ) -> Result<AssignedCell<F, F>, plonk::Error>;

    /// Subtract two field elements and return their difference
    fn sub(
        &self,
        layouter: impl Layouter<F>,
        a: &AssignedCell<F, F>,
        b: &AssignedCell<F, F>,
    ) -> Result<AssignedCell<F, F>, plonk::Error>;

    /// Multiply two field elements and return their product
    fn mul(
        &self,
        layouter: impl Layouter<F>,
        a: &AssignedCell<F, F>,
        b: &AssignedCell<F, F>,
    ) -> Result<AssignedCell<F, F>, plonk::Error>;
}

/// Configuration for the Arithmetic Chip
#[derive(Clone, Debug)]
pub struct ArithConfig {
    /// lhs
    a: Column<Advice>,
    /// rhs
    b: Column<Advice>,
    /// out
    c: Column<Advice>,
    /// Selector for the `add` operation
    q_add: Selector,
    /// Selector for the `sub` operation
    q_sub: Selector,
    /// Selector for the `mul` operation
    q_mul: Selector,
}

/// Arithmetic Chip
pub struct ArithChip<F> {
    config: ArithConfig,
    _marker: PhantomData<F>,
}

impl<F: WithSmallOrderMulGroup<3> + Ord> Chip<F> for ArithChip<F> {
    type Config = ArithConfig;
    type Loaded = ();

    fn config(&self) -> &Self::Config {
        &self.config
    }

    fn loaded(&self) -> &Self::Loaded {
        &()
    }
}

impl<F: WithSmallOrderMulGroup<3> + Ord> ArithChip<F> {
    /// Configure the Arithmetic chip with the given columns
    pub fn configure(
        meta: &mut ConstraintSystem<F>,
        a: Column<Advice>,
        b: Column<Advice>,
        c: Column<Advice>,
    ) -> ArithConfig {
        let q_add = meta.selector();
        let q_sub = meta.selector();
        let q_mul = meta.selector();

        meta.create_gate("Field element addition: c = a + b", |meta| {
            let q_add = meta.query_selector(q_add);
            let a = meta.query_advice(a, Rotation::cur());
            let b = meta.query_advice(b, Rotation::cur());
            let c = meta.query_advice(c, Rotation::cur());

            Constraints::with_selector(q_add, Some(a + b - c))
        });

        meta.create_gate("Field element subtraction: c = a - b", |meta| {
            let q_sub = meta.query_selector(q_sub);
            let a = meta.query_advice(a, Rotation::cur());
            let b = meta.query_advice(b, Rotation::cur());
            let c = meta.query_advice(c, Rotation::cur());

            Constraints::with_selector(q_sub, Some(a - b - c))
        });

        meta.create_gate("Field element multiplication: c = a * b", |meta| {
            let q_mul = meta.query_selector(q_mul);
            let a = meta.query_advice(a, Rotation::cur());
            let b = meta.query_advice(b, Rotation::cur());
            let c = meta.query_advice(c, Rotation::cur());

            Constraints::with_selector(q_mul, Some(a * b - c))
        });

        ArithConfig { a, b, c, q_add, q_sub, q_mul }
    }

    pub fn construct(config: ArithConfig) -> Self {
        Self { config, _marker: PhantomData }
    }
}

impl<F: WithSmallOrderMulGroup<3> + Ord> ArithInstruction<F> for ArithChip<F> {
    fn add(
        &self,
        mut layouter: impl Layouter<F>,
        a: &AssignedCell<F, F>,
        b: &AssignedCell<F, F>,
    ) -> Result<AssignedCell<F, F>, plonk::Error> {
        layouter.assign_region(
            || "c = a + b",
            |mut region| {
                self.config.q_add.enable(&mut region, 0)?;

                a.copy_advice(|| "copy a", &mut region, self.config.a, 0)?;
                b.copy_advice(|| "copy b", &mut region, self.config.b, 0)?;

                let scalar_val = a.value().zip(b.value()).map(|(a, b)| *a + b);
                region.assign_advice(|| "c", self.config.c, 0, || scalar_val)
            },
        )
    }

    fn sub(
        &self,
        mut layouter: impl Layouter<F>,
        a: &AssignedCell<F, F>,
        b: &AssignedCell<F, F>,
    ) -> Result<AssignedCell<F, F>, plonk::Error> {
        layouter.assign_region(
            || "c = a - b",
            |mut region| {
                self.config.q_sub.enable(&mut region, 0)?;

                a.copy_advice(|| "copy a", &mut region, self.config.a, 0)?;
                b.copy_advice(|| "copy b", &mut region, self.config.b, 0)?;

                let scalar_val = a.value().zip(b.value()).map(|(a, b)| *a - b);
                region.assign_advice(|| "c", self.config.c, 0, || scalar_val)
            },
        )
    }

    fn mul(
        &self,
        mut layouter: impl Layouter<F>,
        a: &AssignedCell<F, F>,
        b: &AssignedCell<F, F>,
    ) -> Result<AssignedCell<F, F>, plonk::Error> {
        layouter.assign_region(
            || "c = a * b",
            |mut region| {
                self.config.q_mul.enable(&mut region, 0)?;

                a.copy_advice(|| "copy a", &mut region, self.config.a, 0)?;
                b.copy_advice(|| "copy b", &mut region, self.config.b, 0)?;

                let scalar_val = a.value().zip(b.value()).map(|(a, b)| *a * b);
                region.assign_advice(|| "c", self.config.c, 0, || scalar_val)
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zk::assign_free_advice;
    use darkfi_sdk::pasta::pallas;
    use halo2_proofs::{
        arithmetic::Field,
        circuit::{floor_planner, Value},
        dev::{CircuitLayout, MockProver},
        plonk::{Circuit, Instance as InstanceColumn},
    };

    #[derive(Clone)]
    struct ArithCircuitConfig {
        primary: Column<InstanceColumn>,
        advices: [Column<Advice>; 3],
        arith_config: ArithConfig,
    }

    #[derive(Default)]
    struct ArithCircuit {
        pub one: Value<pallas::Base>,
        pub minus_one: Value<pallas::Base>,
        pub factor: Value<pallas::Base>,
    }

    impl Circuit<pallas::Base> for ArithCircuit {
        type Config = ArithCircuitConfig;
        type FloorPlanner = floor_planner::V1;
        type Params = ();

        fn without_witnesses(&self) -> Self {
            Self::default()
        }

        fn configure(meta: &mut ConstraintSystem<pallas::Base>) -> Self::Config {
            let advices = [meta.advice_column(), meta.advice_column(), meta.advice_column()];

            let primary = meta.instance_column();
            meta.enable_equality(primary);

            for advice in advices.iter() {
                meta.enable_equality(*advice);
            }

            let arith_config = ArithChip::configure(meta, advices[0], advices[1], advices[2]);

            Self::Config { primary, advices, arith_config }
        }

        fn synthesize(
            &self,
            config: Self::Config,
            mut layouter: impl Layouter<pallas::Base>,
        ) -> Result<(), plonk::Error> {
            let arith_chip = ArithChip::construct(config.arith_config.clone());

            let one = assign_free_advice(
                layouter.namespace(|| "Load Fp(1)"),
                config.advices[0],
                self.one,
            )?;

            let minus_one = assign_free_advice(
                layouter.namespace(|| "Load Fp(-1)"),
                config.advices[1],
                self.minus_one,
            )?;

            let factor = assign_free_advice(
                layouter.namespace(|| "Load Fp(factor)"),
                config.advices[2],
                self.factor,
            )?;

            let diff =
                arith_chip.sub(layouter.namespace(|| "one - minus_one"), &one, &minus_one)?;
            layouter.constrain_instance(diff.cell(), config.primary, 0)?;

            let zero =
                arith_chip.add(layouter.namespace(|| "one + minus_one"), &one, &minus_one)?;
            layouter.constrain_instance(zero.cell(), config.primary, 1)?;

            let min_1_min_1 = arith_chip.add(
                layouter.namespace(|| "minus_one + minus_one"),
                &minus_one,
                &minus_one,
            )?;
            layouter.constrain_instance(min_1_min_1.cell(), config.primary, 2)?;

            let product =
                arith_chip.mul(layouter.namespace(|| "minus_one * factor"), &minus_one, &factor)?;
            layouter.constrain_instance(product.cell(), config.primary, 3)?;

            Ok(())
        }
    }

    #[test]
    fn arithmetic_chip() -> crate::Result<()> {
        let one = pallas::Base::ONE;
        let minus_one = -pallas::Base::ONE;
        let factor = pallas::Base::from(644211);

        let public_inputs =
            vec![one - minus_one, pallas::Base::ZERO, minus_one + minus_one, minus_one * factor];

        let circuit = ArithCircuit {
            one: Value::known(one),
            minus_one: Value::known(minus_one),
            factor: Value::known(factor),
        };

        use plotters::prelude::*;
        let root = BitMapBackend::new("target/arithmetic_circuit_layout.png", (3840, 2160))
            .into_drawing_area();
        root.fill(&WHITE).unwrap();
        let root = root.titled("Arithmetic Circuit Layout", ("sans-serif", 60)).unwrap();
        CircuitLayout::default().render(4, &circuit, &root).unwrap();

        let prover = MockProver::run(4, &circuit, vec![public_inputs.clone()])?;
        prover.assert_satisfied();

        Ok(())
    }
}
