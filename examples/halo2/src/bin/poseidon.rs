use std::convert::TryInto;

use halo2::{
    circuit::{floor_planner, Layouter},
    dev::MockProver,
    pasta::{vesta, Fp},
    plonk,
    plonk::{
        Advice, Circuit, Column, ConstraintSystem, Error, Instance as InstanceColumn, Selector,
    },
    poly::{commitment, Rotation},
    transcript::{Blake2bRead, Blake2bWrite},
};

use halo2_examples::circuit::gadget::{
    poseidon::{
        Hash as PoseidonHash, Pow5T3Chip as PoseidonChip, Pow5T3Config as PoseidonConfig,
        StateWord, Word,
    },
    utilities::{copy, CellValue, UtilitiesInstructions, Var},
};
use halo2_examples::primitives::poseidon::{ConstantLength, Hash, OrchardNullifier};

const K: u32 = 6;

#[derive(Clone, Debug)]
struct Config {
    primary: Column<InstanceColumn>,
    q_add: Selector,
    advices: [Column<Advice>; 10],
    poseidon_config: PoseidonConfig<Fp>,
}

#[derive(Default, Debug)]
struct HashCircuit {
    a: Option<Fp>, // First input for hash
    b: Option<Fp>, // Second input for hash
    c: Option<Fp>, // c is summed with hash
}

impl UtilitiesInstructions<Fp> for HashCircuit {
    type Var = CellValue<Fp>;
}

impl Circuit<Fp> for HashCircuit {
    type Config = Config;
    type FloorPlanner = floor_planner::V1;

    fn without_witnesses(&self) -> Self {
        Self::default()
    }

    fn configure(meta: &mut ConstraintSystem<Fp>) -> Self::Config {
        // 10 advice columns
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

        // Addition of two field elements: poseidon_hash(a, b) + c
        let q_add = meta.selector();

        meta.create_gate("poseidon_hash(a, b) + c", |meta| {
            let q_add = meta.query_selector(q_add);
            let sum = meta.query_advice(advices[6], Rotation::cur());
            let hash = meta.query_advice(advices[7], Rotation::cur());
            let c = meta.query_advice(advices[8], Rotation::cur());

            vec![q_add * (hash + c - sum)]
        });

        let primary = meta.instance_column();
        meta.enable_equality(primary.into());

        for advice in advices.iter() {
            meta.enable_equality((*advice).into());
        }

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

        meta.enable_constant(lagrange_coeffs[0]);

        let poseidon_config = PoseidonChip::configure(
            meta,
            OrchardNullifier,
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
        mut layouter: impl Layouter<Fp>,
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
                            config.poseidon_config.state[i],
                            0,
                            || value.ok_or(Error::SynthesisError),
                        )?;
                        region.constrain_equal(var, message[i].cell())?;
                        Ok(Word::<_, _, OrchardNullifier, 3, 2>::from_inner(
                            StateWord::new(var, value),
                        ))
                    };
                    Ok([message_word(0)?, message_word(1)?])
                },
            )?;

            let poseidon_hasher = PoseidonHash::init(
                //config.poseidon_chip(),
                PoseidonChip::construct(config.poseidon_config.clone()),
                layouter.namespace(|| "Poseidon init"),
                ConstantLength::<2>,
            )?;

            let poseidon_output = poseidon_hasher.hash(
                layouter.namespace(|| "Poseidon hash (a, b)"),
                poseidon_message,
            )?;

            let poseidon_output: CellValue<Fp> = poseidon_output.inner().into();
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

        layouter.constrain_instance(scalar.cell(), config.primary, 0)
    }
}

#[derive(Debug)]
struct VerifyingKey {
    params: commitment::Params<vesta::Affine>,
    vk: plonk::VerifyingKey<vesta::Affine>,
}

impl VerifyingKey {
    fn build() -> Self {
        let params = commitment::Params::new(K);
        let circuit: HashCircuit = Default::default();

        let vk = plonk::keygen_vk(&params, &circuit).unwrap();

        VerifyingKey { params, vk }
    }
}

#[derive(Debug)]
struct ProvingKey {
    params: commitment::Params<vesta::Affine>,
    pk: plonk::ProvingKey<vesta::Affine>,
}

impl ProvingKey {
    fn build() -> Self {
        let params = commitment::Params::new(K);
        let circuit: HashCircuit = Default::default();

        let vk = plonk::keygen_vk(&params, &circuit).unwrap();
        let pk = plonk::keygen_pk(&params, vk, &circuit).unwrap();

        ProvingKey { params, pk }
    }
}

#[derive(Clone, Debug)]
struct Proof(Vec<u8>);

impl AsRef<[u8]> for Proof {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Proof {
    fn create(pk: &ProvingKey, circuits: &[HashCircuit], pubinputs: &[Fp]) -> Result<Self, Error> {
        let mut transcript = Blake2bWrite::<_, vesta::Affine, _>::init(vec![]);
        plonk::create_proof(
            &pk.params,
            &pk.pk,
            circuits,
            &[&[pubinputs]],
            &mut transcript,
        )?;
        Ok(Proof(transcript.finalize()))
    }

    fn verify(&self, vk: &VerifyingKey, pubinputs: &[Fp]) -> Result<(), plonk::Error> {
        let msm = vk.params.empty_msm();
        let mut transcript = Blake2bRead::init(&self.0[..]);
        let guard = plonk::verify_proof(&vk.params, &vk.vk, msm, &[&[pubinputs]], &mut transcript)?;
        let msm = guard.clone().use_challenges();
        if msm.eval() {
            Ok(())
        } else {
            Err(Error::ConstraintSystemFailure)
        }
    }

    fn new(bytes: Vec<u8>) -> Self {
        Proof(bytes)
    }
}

fn main() {
    let a = Fp::from(13);
    let b = Fp::from(69);
    let c = Fp::from(42);

    let message = [a, b];
    let output = Hash::init(OrchardNullifier, ConstantLength::<2>).hash(message);

    let circuit = HashCircuit {
        a: Some(a),
        b: Some(b),
        c: Some(c),
    };

    let sum = output + c;

    // Correct:
    let public_inputs = vec![sum + Fp::one()];
    // Incorrect:
    // let public_inputs = vec![sum + Fp::one()];

    let vk = VerifyingKey::build();
    let pk = ProvingKey::build();
    let proof = Proof::create(&pk, &[circuit], &public_inputs).unwrap();
    assert!(proof.verify(&vk, &public_inputs).is_ok());
}
