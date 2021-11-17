use halo2::{
    circuit::{Layouter, SimpleFloorPlanner},
    dev::MockProver,
    plonk::{Advice, Circuit, Column, ConstraintSystem, Error, Instance as InstanceColumn},
};
use halo2_gadgets::{
    poseidon::{Hash as PoseidonHash, Pow5T3Chip as PoseidonChip, Pow5T3Config as PoseidonConfig},
    primitives,
    primitives::poseidon::{ConstantLength, P128Pow5T3},
    utilities::{CellValue, UtilitiesInstructions, Var},
};
use pasta_curves::pallas;

#[derive(Clone)]
struct Config {
    primary: Column<InstanceColumn>,
    advices: [Column<Advice>; 5],
    poseidon_config: PoseidonConfig<pallas::Base>,
}

impl Config {
    fn poseidon_chip(&self) -> PoseidonChip<pallas::Base> {
        PoseidonChip::construct(self.poseidon_config.clone())
    }
}

#[derive(Default)]
struct HashCircuit {
    a: Option<pallas::Base>,
    b: Option<pallas::Base>,
    c: Option<pallas::Base>,
    d: Option<pallas::Base>,
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
        let advices = [
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
        ];

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
            P128Pow5T3,
            advices[1..4].try_into().unwrap(),
            advices[4],
            rc_a,
            rc_b,
        );

        Config {
            primary,
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
        let d = self.load_private(layouter.namespace(|| "load d"), config.advices[0], self.d)?;

        let hash = {
            let poseidon_message = [a, b, c, d];

            let poseidon_hasher = PoseidonHash::<_, _, P128Pow5T3, _, 3, 2>::init(
                config.poseidon_chip(),
                layouter.namespace(|| "Poseidon init"),
                ConstantLength::<4>,
            )?;

            let poseidon_output = poseidon_hasher.hash(
                layouter.namespace(|| "Poseidon hash (a, b)"),
                poseidon_message,
            )?;

            let poseidon_output: CellValue<pallas::Base> = poseidon_output.inner().into();
            poseidon_output
        };

        layouter.constrain_instance(hash.cell(), config.primary, 0)?;

        Ok(())
    }
}

fn main() {
    // The number of rows in our circuit cannot exceed 2^k
    let k: u32 = 9;

    let a = pallas::Base::from(1);
    let b = pallas::Base::from(2);
    let c = pallas::Base::from(3);
    let d = pallas::Base::from(4);

    let message = [a, b, c, d];
    let hash = primitives::poseidon::Hash::init(P128Pow5T3, ConstantLength::<4>).hash(message);

    let circuit = HashCircuit {
        a: Some(a),
        b: Some(b),
        c: Some(c),
        d: Some(d),
    };

    let public_inputs = vec![hash];

    let prover = MockProver::run(k, &circuit, vec![public_inputs]).unwrap();
    assert_eq!(prover.verify(), Ok(()));
}
