use halo2_proofs::{
    circuit::{floor_planner, Layouter, Value},
    dev::MockProver,
    pasta::pallas,
    plonk::{Advice, Circuit, Column, ConstraintSystem, Error},
};

use darkfi::{
    consensus::{types::Float10, utils::fbig2base},
    crypto::{
        proof::{ProvingKey, VerifyingKey},
        Proof,
    },
    zk::gadget::{
        less_than::{LessThanChip, LessThanConfig},
        native_range_check::NativeRangeCheckChip,
    },
};
use log::{error, info};
use rand::rngs::OsRng;

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
            LessThanChip::<WINDOW_SIZE, NUM_BITS, NUM_WINDOWS>::configure(
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
    let y: pallas::Base = pallas::Base::zero();
    let t: pallas::Base = pallas::Base::one();
    let circuit = LessThanCircuit { a: Value::known(y), b: Value::known(t) };

    let prover = MockProver::run(k, &circuit, vec![]).unwrap();
    prover.assert_satisfied();
    assert!(prover.verify().is_ok());

    let public_inputs: Vec<pallas::Base> = vec![];
    let pk = ProvingKey::build(k, &LessThanCircuit::default());
    let vk = VerifyingKey::build(k, &LessThanCircuit::default());
    let proof = Proof::create(&pk, &[circuit], &public_inputs, &mut OsRng)?;
    match proof.verify(&vk, &public_inputs) {
        Ok(()) => {
            info!("proof verified");
            Ok(())
        }
        Err(e) => {
            error!("verification failed: {}", e);
            Err(e)
        }
    }
}

fn fullrange_lessthan(k: u32) -> Result<(), halo2_proofs::plonk::Error> {
    let y_str: &'static str = "0x057eaec1c805d808f70c4e2d2f173c72d091e9c9f78b11dddf52d072c30951ad";
    let t_str: &'static str = "0x2cb8d8aec6766dc83595602e3050b0b908191bbe59dcc3d1e2b7020a37339a14";
    let y: pallas::Base =
        fbig2base(Float10::from_str_native(y_str).unwrap().with_precision(74).value());
    let t: pallas::Base =
        fbig2base(Float10::from_str_native(t_str).unwrap().with_precision(74).value());

    let circuit = LessThanCircuit { a: Value::known(y), b: Value::known(t) };

    let prover = MockProver::run(k, &circuit, vec![]).unwrap();
    prover.assert_satisfied();
    assert!(prover.verify().is_ok());

    let public_inputs: Vec<pallas::Base> = vec![];
    let pk = ProvingKey::build(k, &LessThanCircuit::default());
    let vk = VerifyingKey::build(k, &LessThanCircuit::default());
    let proof = Proof::create(&pk, &[circuit], &public_inputs, &mut OsRng)?;
    match proof.verify(&vk, &public_inputs) {
        Ok(()) => {
            info!("proof verified");
            Ok(())
        }
        Err(e) => {
            error!("verification failed: {}", e);
            Err(e)
        }
    }
}

fn main() {
    env_logger::init();
    let k = 11;
    simple_lessthan(k).unwrap();
    fullrange_lessthan(k).unwrap();
}
