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

use darkfi::zk::assign_free_advice;
use halo2_proofs::{
    arithmetic::Field,
    circuit::{
        //floor_planner::V1,
        Layouter,
        SimpleFloorPlanner,
        Value,
    },
    dev::{CircuitLayout, MockProver},
    pasta::Fp,
    plonk::{self, Advice, Circuit, Column, ConstraintSystem, Instance as InstanceColumn},
};
use plotters::prelude::*;
use rand::rngs::OsRng;

#[derive(Clone)]
struct DynConfig {
    primary: Column<InstanceColumn>,
    advices: Vec<Column<Advice>>,
}

struct DynCircuit {
    pub witnesses: Vec<Value<Fp>>,
}

impl Circuit<Fp> for DynCircuit {
    type Config = DynConfig;
    //type FloorPlanner = V1;
    type FloorPlanner = SimpleFloorPlanner;
    type Params = usize;

    fn without_witnesses(&self) -> Self {
        let mut witnesses = Vec::with_capacity(self.witnesses.len());
        for _ in &self.witnesses {
            witnesses.push(Value::unknown());
        }

        Self { witnesses }
    }

    fn params(&self) -> Self::Params {
        self.witnesses.len()
    }

    fn configure_with_params(
        meta: &mut ConstraintSystem<Fp>,
        params: Self::Params,
    ) -> Self::Config {
        // NOTE: `let advices = vec![meta.advice_column(); params];` does not work as expected.
        let mut advices = vec![];
        for _ in 1..params + 1 {
            advices.push(meta.advice_column());
        }
        for advice in advices.iter() {
            meta.enable_equality(*advice);
        }

        let primary = meta.instance_column();
        meta.enable_equality(primary);

        DynConfig { primary, advices }
    }

    fn configure(_meta: &mut ConstraintSystem<Fp>) -> Self::Config {
        unreachable!();
    }

    fn synthesize(
        &self,
        config: Self::Config,
        mut layouter: impl Layouter<Fp>,
    ) -> Result<(), plonk::Error> {
        for (i, witness) in self.witnesses.iter().enumerate() {
            let w = assign_free_advice(
                layouter.namespace(|| "witness element"),
                config.advices[i],
                *witness,
            )?;

            layouter.constrain_instance(w.cell(), config.primary, i)?;
        }

        Ok(())
    }
}

#[test]
fn dyn_circuit() {
    const ITERS: usize = 10;
    const K: u32 = 4;

    for i in 1..ITERS + 1 {
        let public_inputs = vec![Fp::random(&mut OsRng); i];
        let witnesses = public_inputs.iter().map(|x| Value::known(*x)).collect();
        let circuit = DynCircuit { witnesses };
        let prover = MockProver::run(K, &circuit, vec![public_inputs]).unwrap();
        prover.assert_satisfied();

        let title = format!("target/dynamic_circuit_{i:0>2}.png");
        let root = BitMapBackend::new(&title, (800, 600)).into_drawing_area();
        CircuitLayout::default().render(K, &circuit, &root).unwrap();
    }
}
