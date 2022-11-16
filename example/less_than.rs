/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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

// cargo run --release --example lesthan --all-features
use halo2_proofs::{
    circuit::{floor_planner, Layouter, Value},
    dev::MockProver,
    pasta::{pallas, vesta},
    plonk,
    plonk::{Advice, Circuit, Column, ConstraintSystem, Error, SingleVerifier},
    transcript::{Blake2bRead, Blake2bWrite},
};
use log::{error, info};
use rand::rngs::OsRng;

use darkfi::{
    consensus::{types::Float10, utils::fbig2base, RADIX_BITS},
    crypto::{
        proof::{ProvingKey, VerifyingKey},
        Proof,
    },
    zk::gadget::{
        less_than::{LessThanChip, LessThanConfig},
        native_range_check::NativeRangeCheckChip,
    },
};

const WINDOW_SIZE: usize = 3;
const NUM_BITS: usize = 253;
const NUM_WINDOWS: usize = 85;

#[derive(Default)]
struct LessThanCircuit {
    a: Value<pallas::Base>,
    b: Value<pallas::Base>,
}

impl Circuit<pallas::Base> for LessThanCircuit {
    type Config = (LessThanConfig<WINDOW_SIZE, NUM_BITS, NUM_WINDOWS>, Column<Advice>);
    type FloorPlanner = floor_planner::V1;

    fn without_witnesses(&self) -> Self {
        Self::default()
    }

    fn configure(meta: &mut ConstraintSystem<pallas::Base>) -> Self::Config {
        let advices = [
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
        ];

        for advice in advices.iter() {
            meta.enable_equality(*advice);
        }

        let constants = meta.fixed_column();
        meta.enable_constant(constants);

        let k_values_table = meta.lookup_table_column();

        (
            LessThanChip::<WINDOW_SIZE, NUM_BITS, NUM_WINDOWS>::configure(
                meta,
                advices[1],
                advices[2],
                advices[3],
                advices[4],
                advices[5],
                k_values_table,
            ),
            advices[0],
        )
    }

    fn synthesize(
        &self,
        config: Self::Config,
        mut layouter: impl Layouter<pallas::Base>,
    ) -> Result<(), Error> {
        let less_than_chip =
            LessThanChip::<WINDOW_SIZE, NUM_BITS, NUM_WINDOWS>::construct(config.0.clone());

        NativeRangeCheckChip::<WINDOW_SIZE, NUM_BITS, NUM_WINDOWS>::load_k_table(
            &mut layouter,
            config.0.k_values_table,
        )?;

        less_than_chip.witness_less_than(
            layouter.namespace(|| "a < b"),
            self.a,
            self.b,
            0,
            true,
        )?;

        Ok(())
    }
}

fn simple_lessthan(k: u32) -> Result<(), halo2_proofs::plonk::Error> {
    let circuit = LessThanCircuit {
        a: Value::known(pallas::Base::from(0)),
        b: Value::known(pallas::Base::from(1)),
    };

    let prover = MockProver::run(k, &circuit, vec![]).unwrap();
    prover.assert_satisfied();

    // Prover:
    let pk = ProvingKey::build(k, &LessThanCircuit::default());
    let mut transcript = Blake2bWrite::<_, vesta::Affine, _>::init(vec![]);
    plonk::create_proof(&pk.params, &pk.pk, &[circuit], &[&[]], &mut OsRng, &mut transcript)?;
    let proof = transcript.finalize();

    // Verifier:
    let vk = VerifyingKey::build(k, &LessThanCircuit::default());
    let strategy = SingleVerifier::new(&vk.params);
    let mut transcript = Blake2bRead::init(&proof[..]);
    plonk::verify_proof(&vk.params, &vk.vk, strategy, &[&[]], &mut transcript)?;

    Ok(())
}

fn fullrange_lessthan(k: u32) -> Result<(), halo2_proofs::plonk::Error> {
    let y_str: &'static str =
        "2485393101277319054866673974886504690592360759087472860138246047042221199789";
    let t_str: &'static str =
        "20228360686725123198855333388287068776098384779255635716769234906173337213460";
    let y: pallas::Base =
        fbig2base(Float10::from_str_native(y_str).unwrap().with_precision(*RADIX_BITS).value());
    let t: pallas::Base =
        fbig2base(Float10::from_str_native(t_str).unwrap().with_precision(*RADIX_BITS).value());

    let circuit = LessThanCircuit { a: Value::known(y), b: Value::known(t) };

    let prover = MockProver::run(k, &circuit, vec![]).unwrap();
    prover.assert_satisfied();
    assert!(prover.verify().is_ok());

    let public_inputs: Vec<pallas::Base> = vec![];
    let pk = ProvingKey::build(k, &LessThanCircuit::default());
    let vk = VerifyingKey::build(k, &LessThanCircuit::default());
    let mut transcript = Blake2bWrite::<_, vesta::Affine, _>::init(vec![]);
    plonk::create_proof(&pk.params, &pk.pk, &[circuit], &[&[]], &mut OsRng, &mut transcript)?;
    let proof = transcript.finalize();
    let strategy = SingleVerifier::new(&vk.params);
    let mut transcript = Blake2bRead::init(&proof[..]);
    plonk::verify_proof(&vk.params, &vk.vk, strategy, &[&[]], &mut transcript)?;

    Ok(())
}

fn main() {
    env_logger::init();
    let k = 11;
    simple_lessthan(k).unwrap();
    fullrange_lessthan(k).unwrap();
}
