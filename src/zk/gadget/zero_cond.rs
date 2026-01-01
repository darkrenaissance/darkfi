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

use halo2_proofs::{
    circuit::{AssignedCell, Layouter},
    pasta::group::ff::WithSmallOrderMulGroup,
    plonk::{Advice, Column, ConstraintSystem, Error, Expression, Selector},
    poly::Rotation,
};

use super::is_zero::{IsZeroChip, IsZeroConfig};

#[derive(Clone, Debug)]
pub struct ZeroCondConfig<F> {
    selector: Selector,
    a: Column<Advice>,
    b: Column<Advice>,
    is_zero: IsZeroConfig<F>,
    output: Column<Advice>,
}

#[derive(Clone, Debug)]
pub struct ZeroCondChip<F: WithSmallOrderMulGroup<3> + Ord> {
    config: ZeroCondConfig<F>,
}

impl<F: WithSmallOrderMulGroup<3> + Ord> ZeroCondChip<F> {
    pub fn construct(config: ZeroCondConfig<F>) -> Self {
        Self { config }
    }

    /// Configure the chip.
    ///
    /// Advice columns:
    /// * `[0]` - a
    /// * `[1]` - b
    /// * `[2]` - is_zero output
    /// * `[3]` - zero_cond output
    pub fn configure(
        meta: &mut ConstraintSystem<F>,
        advices: [Column<Advice>; 4],
    ) -> ZeroCondConfig<F> {
        for i in advices {
            meta.enable_equality(i);
        }

        let selector = meta.selector();

        let is_zero = IsZeroChip::configure(
            meta,
            |meta| meta.query_selector(selector),
            |meta| meta.query_advice(advices[0], Rotation::cur()),
            advices[2],
        );

        // NOTE: a is not used here because it already went into IsZero
        meta.create_gate("f(a, b) = if a == 0 {a} else {b}", |meta| {
            let s = meta.query_selector(selector);
            let b = meta.query_advice(advices[1], Rotation::cur());
            let output = meta.query_advice(advices[3], Rotation::cur());

            let one = Expression::Constant(F::ONE);

            vec![s * (is_zero.expr() * output.clone() + (one - is_zero.expr()) * (output - b))]
        });

        ZeroCondConfig { selector, a: advices[0], b: advices[1], is_zero, output: advices[3] }
    }

    pub fn assign(
        &self,
        mut layouter: impl Layouter<F>,
        a: AssignedCell<F, F>,
        b: AssignedCell<F, F>,
    ) -> Result<AssignedCell<F, F>, Error> {
        let is_zero_chip = IsZeroChip::construct(self.config.is_zero.clone());

        let out = layouter.assign_region(
            || "f(a, b) = if a == 0 {a} else {b}",
            |mut region| {
                self.config.selector.enable(&mut region, 0)?;
                let a = a.copy_advice(|| "copy a", &mut region, self.config.a, 0)?;
                let b = b.copy_advice(|| "copy b", &mut region, self.config.b, 0)?;
                is_zero_chip.assign(&mut region, 0, a.value().copied())?;

                let output = a.value().copied().to_field().zip(b.value().copied()).map(|(a, b)| {
                    if a == F::ZERO.into() {
                        F::ZERO
                    } else {
                        b
                    }
                });

                let cell = region.assign_advice(|| "output", self.config.output, 0, || output)?;
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
        circuit::{SimpleFloorPlanner, Value},
        dev::MockProver,
        pasta::Fp,
        plonk::{Circuit, Instance},
    };

    #[derive(Default)]
    struct MyCircuit {
        a: Value<Fp>,
        b: Value<Fp>,
    }

    impl Circuit<Fp> for MyCircuit {
        type Config = (ZeroCondConfig<Fp>, [Column<Advice>; 5], Column<Instance>);
        type FloorPlanner = SimpleFloorPlanner;
        type Params = ();

        fn without_witnesses(&self) -> Self {
            Self::default()
        }

        fn configure(meta: &mut ConstraintSystem<Fp>) -> Self::Config {
            let advices = [
                meta.advice_column(),
                meta.advice_column(),
                meta.advice_column(),
                meta.advice_column(),
                meta.advice_column(),
            ];
            for i in advices {
                meta.enable_equality(i);
            }

            let instance = meta.instance_column();
            meta.enable_equality(instance);

            let zcc = ZeroCondChip::configure(meta, advices[1..5].try_into().unwrap());

            (zcc, advices, instance)
        }

        fn synthesize(
            &self,
            config: Self::Config,
            mut layouter: impl Layouter<Fp>,
        ) -> Result<(), Error> {
            let a = assign_free_advice(layouter.namespace(|| "load a"), config.1[0], self.a)?;
            let b = assign_free_advice(layouter.namespace(|| "load b"), config.1[0], self.b)?;

            let zcc = ZeroCondChip::construct(config.0);
            let output = zcc.assign(layouter.namespace(|| "zero_cond"), a, b)?;
            layouter.constrain_instance(output.cell(), config.2, 0)?;

            Ok(())
        }
    }

    #[test]
    fn zero_cond() {
        let a = Fp::from(0);
        let b = Fp::from(69);
        let p_circuit = MyCircuit { a: Value::known(a), b: Value::known(b) };
        let public_inputs = vec![a];
        let prover = MockProver::run(3, &p_circuit, vec![public_inputs]).unwrap();
        prover.assert_satisfied();

        let a = Fp::from(12);
        let b = Fp::from(42);
        let p_circuit = MyCircuit { a: Value::known(a), b: Value::known(b) };
        let public_inputs = vec![b];
        let prover = MockProver::run(3, &p_circuit, vec![public_inputs]).unwrap();
        prover.assert_satisfied();
    }
}
