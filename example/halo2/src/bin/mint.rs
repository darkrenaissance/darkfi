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
    ecc::{
        chip::{EccChip, EccConfig},
        FixedPoint,
    },
    poseidon::{
        Hash as PoseidonHash, Pow5T3Chip as PoseidonChip, Pow5T3Config as PoseidonConfig,
        StateWord, Word,
    },
    primitives,
    primitives::poseidon::{ConstantLength, P128Pow5T3},
    sinsemilla::{
        chip::{SinsemillaChip, SinsemillaConfig},
        merkle::chip::{MerkleChip, MerkleConfig},
    },
    utilities::{
        copy, lookup_range_check::LookupRangeCheckConfig, CellValue, UtilitiesInstructions, Var,
    },
};
use pasta_curves::{
    arithmetic::{CurveAffine, Field},
    group::{Curve, Group},
    pallas,
};
use rand::rngs::OsRng;

use drk_halo2::{
    constants::{
        sinsemilla::{OrchardCommitDomains, OrchardHashDomains},
        OrchardFixedBases,
    },
    crypto::pedersen_commitment,
    proof::{Proof, ProvingKey, VerifyingKey},
};

#[derive(Clone, Debug)]
struct MintConfig {
    primary: Column<InstanceColumn>,
    q_add: Selector,
    advices: [Column<Advice>; 10],
    ecc_config: EccConfig,
    merkle_config_1: MerkleConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    merkle_config_2: MerkleConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    sinsemilla_config_1:
        SinsemillaConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    sinsemilla_config_2:
        SinsemillaConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    poseidon_config: PoseidonConfig<pallas::Base>,
}

impl MintConfig {
    fn ecc_chip(&self) -> EccChip<OrchardFixedBases> {
        EccChip::construct(self.ecc_config.clone())
    }

    fn poseidon_chip(&self) -> PoseidonChip<pallas::Base> {
        PoseidonChip::construct(self.poseidon_config.clone())
    }
}

// The public input array offsets
const MINT_COIN_OFFSET: usize = 0;
const MINT_VALCOMX_OFFSET: usize = 1;
const MINT_VALCOMY_OFFSET: usize = 2;
const MINT_ASSCOMX_OFFSET: usize = 3;
const MINT_ASSCOMY_OFFSET: usize = 4;

#[derive(Default, Debug)]
struct MintCircuit {
    pub_x: Option<pallas::Base>,         // x coordinate for pubkey
    pub_y: Option<pallas::Base>,         // y coordinate for pubkey
    value: Option<pallas::Base>,         // The value of this coin
    asset: Option<pallas::Base>,         // The asset ID
    serial: Option<pallas::Base>,        // Unique serial number corresponding to this coin
    coin_blind: Option<pallas::Base>,    // Random blinding factor for coin
    value_blind: Option<pallas::Scalar>, // Random blinding factor for value commitment
    asset_blind: Option<pallas::Scalar>, // Random blinding factor for the asset ID
}

impl UtilitiesInstructions<pallas::Base> for MintCircuit {
    type Var = CellValue<pallas::Base>;
}

impl Circuit<pallas::Base> for MintCircuit {
    type Config = MintConfig;
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
        /*
        let q_add = meta.selector();
        meta.create_gate("poseidon_hash(a, b) + c", |meta| {
            let q_add = meta.query_selector(q_add);
            let sum = meta.query_advice(advices[6], Rotation::cur());
            let hash = meta.query_advice(advices[7], Rotation::cur());
            let c = meta.query_advice(advices[8], Rotation::cur());

            vec![q_add * (hash + c - sum)]
        });
        */
        let q_add = meta.selector();
        meta.create_gate("a+b+c", |meta| {
            let q_add = meta.query_selector(q_add);
            let sum = meta.query_advice(advices[5], Rotation::cur());
            let a = meta.query_advice(advices[6], Rotation::cur());
            let b = meta.query_advice(advices[7], Rotation::cur());
            let c = meta.query_advice(advices[8], Rotation::cur());

            vec![q_add * (a + b + c - sum)]
        });

        // Fixed columns for the Sinsemilla generator lookup table
        let table_idx = meta.lookup_table_column();
        let lookup = (
            table_idx,
            meta.lookup_table_column(),
            meta.lookup_table_column(),
        );

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

        // Use one of the right-most advice columns for all of our range checks.
        let range_check = LookupRangeCheckConfig::configure(meta, advices[9], table_idx);

        // Configuration for curve point operations.
        // This uses 10 advice columns and spans the whole circuit.
        let ecc_config = EccChip::<OrchardFixedBases>::configure(
            meta,
            advices,
            lagrange_coeffs,
            range_check.clone(),
        );

        // Configuration for the Poseidon hash
        let poseidon_config = PoseidonChip::configure(
            meta,
            P128Pow5T3,
            advices[6..9].try_into().unwrap(),
            advices[5],
            rc_a,
            rc_b,
        );

        // Configuration for a Sinsemilla hash instantiation and a
        // Merkle hash instantiation using this Sinsemilla instance.
        // Since the Sinsemilla config uses only 5 advice columns,
        // we can fit two instances side-by-side.
        let (sinsemilla_config_1, merkle_config_1) = {
            let sinsemilla_config_1 = SinsemillaChip::configure(
                meta,
                advices[..5].try_into().unwrap(),
                advices[6],
                lagrange_coeffs[0],
                lookup,
                range_check.clone(),
            );
            let merkle_config_1 = MerkleChip::configure(meta, sinsemilla_config_1.clone());
            (sinsemilla_config_1, merkle_config_1)
        };

        // Configuration for a Sinsemilla hash instantiation and a
        // Merkle hash instantiation using this Sinsemilla instance.
        // Since the Sinsemilla config uses only 5 advice columns,
        // we can fit two instances side-by-side.
        let (sinsemilla_config_2, merkle_config_2) = {
            let sinsemilla_config_2 = SinsemillaChip::configure(
                meta,
                advices[5..].try_into().unwrap(),
                advices[7],
                lagrange_coeffs[1],
                lookup,
                range_check,
            );
            let merkle_config_2 = MerkleChip::configure(meta, sinsemilla_config_2.clone());

            (sinsemilla_config_2, merkle_config_2)
        };

        MintConfig {
            primary,
            q_add,
            advices,
            ecc_config,
            merkle_config_1,
            merkle_config_2,
            sinsemilla_config_1,
            sinsemilla_config_2,
            poseidon_config,
        }
    }

    fn synthesize(
        &self,
        config: Self::Config,
        mut layouter: impl Layouter<pallas::Base>,
    ) -> Result<(), Error> {
        // Load the Sinsemilla generator lookup table used by the whole circuit.
        SinsemillaChip::load(config.sinsemilla_config_1.clone(), &mut layouter)?;

        let ecc_chip = config.ecc_chip();

        let pub_x = self.load_private(
            layouter.namespace(|| "load pubkey x"),
            config.advices[0],
            self.pub_x,
        )?;

        let pub_y = self.load_private(
            layouter.namespace(|| "load pubkey y"),
            config.advices[0],
            self.pub_y,
        )?;

        let value = self.load_private(
            layouter.namespace(|| "load value"),
            config.advices[0],
            self.value,
        )?;

        let asset = self.load_private(
            layouter.namespace(|| "load asset"),
            config.advices[0],
            self.asset,
        )?;

        let serial = self.load_private(
            layouter.namespace(|| "load serial"),
            config.advices[0],
            self.serial,
        )?;

        let coin_blind = self.load_private(
            layouter.namespace(|| "load coin_blind"),
            config.advices[0],
            self.coin_blind,
        )?;

        // =========
        // Coin hash
        // =========
        let messages = [[pub_x, pub_y], [value, asset], [serial, coin_blind]];
        let mut hashes = vec![];

        for message in messages.iter() {
            let hash = {
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

            hashes.push(hash);
        }

        let coin = layouter.assign_region(
            || " `coin` = hash(a,b) + hash(c, d) + hash(e, f)",
            |mut region| {
                config.q_add.enable(&mut region, 0)?;

                copy(&mut region, || "copy ab", config.advices[6], 0, &hashes[0])?;
                copy(&mut region, || "copy cd", config.advices[7], 0, &hashes[1])?;
                copy(&mut region, || "copy ef", config.advices[8], 0, &hashes[2])?;

                let scalar_val = hashes[0]
                    .value()
                    .zip(hashes[1].value())
                    .zip(hashes[2].value())
                    .map(|(abcd, ef)| abcd.0 + abcd.1 + ef);

                let cell = region.assign_advice(
                    || "hash(a,b)+hash(c,d)+hash(e,f)",
                    config.advices[5],
                    0,
                    || scalar_val.ok_or(Error::SynthesisError),
                )?;
                Ok(CellValue::new(cell, scalar_val))
            },
        )?;

        // Constrain the coin C
        layouter.constrain_instance(coin.cell(), config.primary, MINT_COIN_OFFSET)?;

        // ================
        // Value commitment
        // ================

        // This constant one is used for short multiplication
        let one = self.load_private(
            layouter.namespace(|| "load constant one"),
            config.advices[0],
            Some(pallas::Base::one()),
        )?;

        // v * G_1
        let (commitment, _) = {
            let value_commit_v = OrchardFixedBases::ValueCommitV;
            let value_commit_v = FixedPoint::from_inner(ecc_chip.clone(), value_commit_v);
            value_commit_v.mul_short(layouter.namespace(|| "[value] ValueCommitV"), (value, one))?
        };

        // r_V * G_2
        let (blind, _rcv) = {
            let rcv = self.value_blind;
            let value_commit_r = OrchardFixedBases::ValueCommitR;
            let value_commit_r = FixedPoint::from_inner(ecc_chip.clone(), value_commit_r);
            value_commit_r.mul(layouter.namespace(|| "[value_blind] ValueCommitR"), rcv)?
        };

        // Constrain the value commitment coordinates
        let value_commit = commitment.add(layouter.namespace(|| "valuecommit"), &blind)?;
        layouter.constrain_instance(
            value_commit.inner().x().cell(),
            config.primary,
            MINT_VALCOMX_OFFSET,
        )?;
        layouter.constrain_instance(
            value_commit.inner().y().cell(),
            config.primary,
            MINT_VALCOMY_OFFSET,
        )?;

        // ================
        // Asset commitment
        // ================
        // a * G_1
        let (commitment, _) = {
            let asset_commit_v = OrchardFixedBases::ValueCommitV;
            let asset_commit_v = FixedPoint::from_inner(ecc_chip.clone(), asset_commit_v);
            asset_commit_v.mul_short(layouter.namespace(|| "[asset] ValueCommitV"), (asset, one))?
        };

        // r_A * G_2
        let (blind, _rca) = {
            let rca = self.asset_blind;
            let asset_commit_r = OrchardFixedBases::ValueCommitR;
            let asset_commit_r = FixedPoint::from_inner(ecc_chip, asset_commit_r);
            asset_commit_r.mul(layouter.namespace(|| "[asset_blind] ValueCommitR"), rca)?
        };

        // Constrain the asset commitment coordinates
        let asset_commit = commitment.add(layouter.namespace(|| "assetcommit"), &blind)?;
        layouter.constrain_instance(
            asset_commit.inner().x().cell(),
            config.primary,
            MINT_ASSCOMX_OFFSET,
        )?;
        layouter.constrain_instance(
            asset_commit.inner().y().cell(),
            config.primary,
            MINT_ASSCOMY_OFFSET,
        )?;

        // At this point we've enforced all of our public inputs.
        Ok(())
    }
}

fn main() {
    // The number of rows in our circuit cannot exceed 2^k
    let k: u32 = 11;

    let pubkey = pallas::Point::random(&mut OsRng);
    let coords = pubkey.to_affine().coordinates().unwrap();

    let value = 42;
    let asset = 1;

    let value_blind = pallas::Scalar::random(&mut OsRng);
    let asset_blind = pallas::Scalar::random(&mut OsRng);

    let serial = pallas::Base::random(&mut OsRng);
    let coin_blind = pallas::Base::random(&mut OsRng);

    // poseidon_hash(x, y) + poseidon_hash(value, asset) + poseidon_hash(serial, coin_blind)
    let mut coin = pallas::Base::zero();

    let messages = [
        [*coords.x(), *coords.y()],
        [pallas::Base::from(value), pallas::Base::from(asset)],
        [serial, coin_blind],
    ];

    for msg in messages.iter() {
        let hash = primitives::poseidon::Hash::init(P128Pow5T3, ConstantLength::<2>).hash(*msg);
        coin += hash;
    }

    let value_commit = pedersen_commitment(value, value_blind);
    let value_coords = value_commit.to_affine().coordinates().unwrap();

    let asset_commit = pedersen_commitment(asset, asset_blind);
    let asset_coords = asset_commit.to_affine().coordinates().unwrap();

    let public_inputs = vec![
        coin,
        *value_coords.x(),
        *value_coords.y(),
        *asset_coords.x(),
        *asset_coords.y(),
    ];

    let circuit = MintCircuit {
        pub_x: Some(*coords.x()),
        pub_y: Some(*coords.y()),
        value: Some(pallas::Base::from(value)),
        asset: Some(pallas::Base::from(asset)),
        serial: Some(serial),
        coin_blind: Some(coin_blind),
        value_blind: Some(value_blind),
        asset_blind: Some(asset_blind),
    };

    let prover = MockProver::run(k, &circuit, vec![public_inputs.clone()]).unwrap();
    assert_eq!(prover.verify(), Ok(()));

    // Actual ZK proof
    let start = Instant::now();
    let vk = VerifyingKey::build(k, MintCircuit::default());
    let pk = ProvingKey::build(k, MintCircuit::default());
    println!("Setup: [{:?}]", start.elapsed());

    let start = Instant::now();
    let proof = Proof::create(&pk, &[circuit], &public_inputs).unwrap();
    println!("Prove: [{:?}]", start.elapsed());

    let start = Instant::now();
    assert!(proof.verify(&vk, &public_inputs).is_ok());
    println!("Verify: [{:?}]", start.elapsed());
}
