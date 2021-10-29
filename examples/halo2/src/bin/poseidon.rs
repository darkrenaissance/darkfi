use std::time::Instant;

use halo2::{
    circuit::{Layouter, SimpleFloorPlanner},
    dev::MockProver,
    plonk::{
        Advice, Circuit, Column, ConstraintSystem, Error, Instance as InstanceColumn, Selector,
    },
    poly::Rotation,
};
use halo2_gadgets::{
    poseidon::{
        Hash as PoseidonHash, Pow5T3Chip as PoseidonChip, Pow5T3Config as PoseidonConfig,
        StateWord, Word,
    },
    primitives,
    primitives::poseidon::{ConstantLength, P128Pow5T3},
    utilities::{copy, CellValue, UtilitiesInstructions, Var},
};
use pasta_curves::pallas;

use drk_halo2::{Proof, ProvingKey, VerifyingKey};

#[derive(Clone, Debug)]
struct Config {
    primary: Column<InstanceColumn>,
    q_add: Selector,
    advices: [Column<Advice>; 10],
    poseidon_config: PoseidonConfig<pallas::Base>,
}

impl Config {
    fn poseidon_chip(&self) -> PoseidonChip<pallas::Base> {
        PoseidonChip::construct(self.poseidon_config.clone())
    }
}

#[derive(Default, Debug)]
struct HashCircuit {
    a: Option<pallas::Base>,
    b: Option<pallas::Base>,
    c: Option<pallas::Base>,
}

impl UtilitiesInstructions<pallas::Base> for HashCircuit {
    type Var = CellValue<pallas::Base>;
}

impl Circuit<pallas::Base> for HashCircuit {
    type Config = Config;
    type FloorPlanner = SimpleFloorPlanner;

    fn without_witnesses(&self) -> Self {
        Self::default()
    }

    fn configure(meta: &mut ConstraintSystem<pallas::Base>) -> Self::Config {
        // Advice columns used in the circuit
        let advices = [
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
        ];

        // Addition of two field elements
        let q_add = meta.selector();
        meta.create_gate("poseidon_hash(a, b) + c", |meta| {
            let q_add = meta.query_selector(q_add);
            let sum = meta.query_advice(advices[6], Rotation::cur());
            let hash = meta.query_advice(advices[7], Rotation::cur());
            let c = meta.query_advice(advices[8], Rotation::cur());

            vec![q_add * (hash + c - sum)]
        });

        // Instance column used for public inputs
        let primary = meta.instance_column();
        meta.enable_equality(primary.into());

        // Permutation over all advice columns
        for advice in advices.iter() {
            meta.enable_equality((*advice).into());
        }

        // Poseidon requires four advice columns, while ECC incomplete addition
        // requires six. We can reduce the proof size by sharing fixed columns
        // between the ECC and Poseidon chips.
        // TODO: For multiple invocations they could/should be configured in
        // parallel rather than sharing perhaps?
        let lagrange_coeffs = [
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
        ];
        let rc_a = lagrange_coeffs[2..5].try_into().unwrap();
        let rc_b = lagrange_coeffs[5..8].try_into().unwrap();

        // Also use the first Lagrange coefficient column for loading global constants.
        meta.enable_constant(lagrange_coeffs[0]);

        // Configuration for the Poseidon hash
        let poseidon_config = PoseidonChip::configure(
            meta,
            P128Pow5T3,
            advices[6..9].try_into().unwrap(),
            advices[5],
            rc_a,
            rc_b,
        );

        Config {
            primary,
            q_add,
            advices,
            poseidon_config,
        }
    }

    fn synthesize(
        &self,
        config: Self::Config,
        mut layouter: impl Layouter<pallas::Base>,
    ) -> Result<(), Error> {
        let a = self.load_private(layouter.namespace(|| "load a"), config.advices[0], self.a)?;
        let b = self.load_private(layouter.namespace(|| "load b"), config.advices[0], self.b)?;
        let c = self.load_private(layouter.namespace(|| "load c"), config.advices[0], self.c)?;

        let hash = {
            let message = [a, b];

            let poseidon_message = layouter.assign_region(
                || "load message",
                |mut region| {
                    let mut message_word = |i: usize| {
                        let value = message[i].value();
                        let var = region.assign_advice(
                            || format!("load message_{}", i),
                            config.poseidon_config.state()[i],
                            0,
                            || value.ok_or(Error::SynthesisError),
                        )?;
                        region.constrain_equal(var, message[i].cell())?;
                        Ok(Word::<_, _, P128Pow5T3, 3, 2>::from_inner(StateWord::new(
                            var, value,
                        )))
                    };
                    Ok([message_word(0)?, message_word(1)?])
                },
            )?;

            let poseidon_hasher = PoseidonHash::init(
                config.poseidon_chip(),
                layouter.namespace(|| "Poseidon init"),
                ConstantLength::<2>,
            )?;

            let poseidon_output = poseidon_hasher.hash(
                layouter.namespace(|| "Poseidon hash (a, b)"),
                poseidon_message,
            )?;

            let poseidon_output: CellValue<pallas::Base> = poseidon_output.inner().into();
            poseidon_output
        };

        // Add hash output to c
        let scalar = layouter.assign_region(
            || " `scalar` = poseidon_hash(a, b) + c",
            |mut region| {
                config.q_add.enable(&mut region, 0)?;

                copy(&mut region, || "copy hash", config.advices[7], 0, &hash)?;
                copy(&mut region, || "copy c", config.advices[8], 0, &c)?;

                let scalar_val = hash.value().zip(c.value()).map(|(hash, c)| hash + c);

                let cell = region.assign_advice(
                    || "poseidon_hash(a, b) + c",
                    config.advices[6],
                    0,
                    || scalar_val.ok_or(Error::SynthesisError),
                )?;
                Ok(CellValue::new(cell, scalar_val))
            },
        )?;

        // Constrain sum to equal the public input
        layouter.constrain_instance(scalar.cell(), config.primary, 0)?;

        // At this point we've enforced all of our public inputs.
        Ok(())
    }
}

fn main() {
    // The number of rows in our circuit cannot exceed 2^k
    let k: u32 = 6;

    let a = pallas::Base::from(13);
    let b = pallas::Base::from(69);
    let c = pallas::Base::from(42);

    let message = [a, b];
    let output = primitives::poseidon::Hash::init(P128Pow5T3, ConstantLength::<2>).hash(message);

    let circuit = HashCircuit {
        a: Some(a),
        b: Some(b),
        c: Some(c),
    };

    let sum = output + c;

    // Incorrect:
    let public_inputs = vec![sum + pallas::Base::one()];
    let prover = MockProver::run(k, &circuit, vec![public_inputs]).unwrap();
    assert!(prover.verify().is_err());

    // Correct:
    let public_inputs = vec![sum];
    let prover = MockProver::run(k, &circuit, vec![public_inputs.clone()]).unwrap();
    assert_eq!(prover.verify(), Ok(()));

    // Actual ZK proof
    let start = Instant::now();
    let vk = VerifyingKey::build(k, HashCircuit::default());
    let pk = ProvingKey::build(k, HashCircuit::default());
    println!("Setup: [{:?}]", start.elapsed());

    let start = Instant::now();
    let proof = Proof::create(&pk, &[circuit], &public_inputs).unwrap();
    println!("Prove: [{:?}]", start.elapsed());

    let start = Instant::now();
    assert!(proof.verify(&vk, &public_inputs).is_ok());
    println!("Verify: [{:?}]", start.elapsed());
}
