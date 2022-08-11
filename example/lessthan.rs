use halo2_proofs::{
    circuit::{AssignedCell, Layouter, SimpleFloorPlanner, floor_planner, Value},
    plonk::{Advice, Circuit, Column, ConstraintSystem, Error, Instance as InstanceColumn},
    dev::{CircuitLayout, MockProver},
    pasta::pallas,
};

use darkfi:: {
    zk::gadget:: {
        less_than::{ LessThanConfig, LessThanChip},
        native_range_check::{NativeRangeCheckChip},
    },
};

#[derive(Default)]
struct LessThanCircuit {
    a: Value<pallas::Base>,
    b: Value<pallas::Base>,
}

const WINDOW_SIZE: usize = 3;
const NUM_OF_BITS: usize = 254;
const NUM_OF_WINDOWS: usize = 85;


impl Circuit<pallas::Base> for LessThanCircuit {
    type Config =
        (LessThanConfig<WINDOW_SIZE, NUM_OF_BITS, NUM_OF_WINDOWS>, Column<Advice>);
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

        let k_values_table = meta.lookup_table_column();

        let constants = meta.fixed_column();
        meta.enable_constant(constants);

        (
            LessThanChip::<WINDOW_SIZE, NUM_OF_BITS, NUM_OF_WINDOWS>::configure(
                meta,
                a,
                b,
                a_offset,
                k_values_table,
            ),
            w,
        )
    }

    fn synthesize (
        &self,
        config: Self::Config,
        mut layouter: impl Layouter<pallas::Base>,
    ) -> Result<(), Error> {
        let less_than_chip =
            LessThanChip::<WINDOW_SIZE, NUM_OF_BITS, NUM_OF_WINDOWS>::construct(
                config.0.clone(),
            );

        NativeRangeCheckChip::<WINDOW_SIZE, NUM_OF_BITS, NUM_OF_WINDOWS>::load_k_table(
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

fn main() {
    let k = 13;
    let valid_a_vals = vec![
        pallas::Base::from(4),
        pallas::Base::zero(),
        pallas::Base::one()
    ];
    let valid_b_vals = vec![
        pallas::Base::from(5),
        pallas::Base::from(u64::MAX),
        pallas::Base::from(rand::random::<u64>()),
    ];


    let invalid_a_vals = vec![
        pallas::Base::from(14),
        pallas::Base::from(u64::MAX),
        pallas::Base::zero(),
        pallas::Base::one(),
        pallas::Base::from(u64::MAX),
    ];
    let invalid_b_vals = vec![
        pallas::Base::from(11),
        pallas::Base::zero(),
        pallas::Base::zero(),
        pallas::Base::one(),
        pallas::Base::from(u64::MAX),
    ];
    use plotters::prelude::*;
    let circuit = LessThanCircuit {
        a: Value::known(pallas::Base::zero()),
        b: Value::known(pallas::Base::one()),
    };
    let root = BitMapBackend::new("target/lessthan_circuit_layout.png", (3840, 2160))
        .into_drawing_area();
    CircuitLayout::default().render(k, &circuit, &root).unwrap();

    let one = pallas::Base::one();
    let zero = pallas::Base::zero();
    let public_inputs = vec![one];

    for i in 0..valid_a_vals.len() {
        let a = valid_a_vals[i];
        let b = valid_b_vals[i];

        println!("64 bit (valid) {:?} < {:?} check", a, b);

        let circuit = LessThanCircuit { a: Value::known(a), b: Value::known(b) };

        let prover = MockProver::run(k, &circuit, public_inputs).unwrap();
        prover.assert_satisfied();
    }


    for i in 0..invalid_a_vals.len() {
        let a = invalid_a_vals[i];
        let b = invalid_b_vals[i];

        println!("64 bit (invalid) {:?} < {:?} check", a, b);

        let circuit = LessThanCircuit { a: Value::known(a), b: Value::known(b) };

        let prover = MockProver::run(k, &circuit, public_inputs).unwrap();
        assert!(prover.verify().is_err())
    }

}
