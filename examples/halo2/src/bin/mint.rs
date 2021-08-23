use std::{convert::TryInto, time::Instant};

use group::{ff::Field, Curve, Group};
use halo2::{
    arithmetic::{CurveAffine, CurveExt, FieldExt},
    circuit::{floor_planner, Layouter},
    dev::MockProver,
    pasta::{vesta, Ep, Fp, Fq},
    plonk,
    plonk::{Circuit, ConstraintSystem, Error},
    poly::commitment,
    transcript::{Blake2bRead, Blake2bWrite},
};
use halo2_ecc::{chip::EccChip, gadget::FixedPoint};
use halo2_poseidon::{
    gadget::{Hash as PoseidonHash, Word},
    pow5t3::{Pow5T3Chip as PoseidonChip, StateWord},
    primitive::{ConstantLength, Hash, P128Pow5T3 as OrchardNullifier},
};
use halo2_utilities::{
    lookup_range_check::LookupRangeCheckConfig, CellValue, UtilitiesInstructions, Var,
};
use orchard::constants::fixed_bases::{
    OrchardFixedBases, VALUE_COMMITMENT_PERSONALIZATION, VALUE_COMMITMENT_R_BYTES,
    VALUE_COMMITMENT_V_BYTES,
};
use rand::rngs::OsRng;

use halo2_examples::circuit::Config;

const K: u32 = 9;

#[derive(Default, Debug)]
struct MintCircuit {
    pub_x: Option<Fp>,       // x coordinate for pubkey
    pub_y: Option<Fp>,       // y coordinate for pubkey
    value: Option<Fp>,       // The value of this coin
    asset: Option<Fp>,       // The asset ID
    serial: Option<Fp>,      // Unique serial number corresponding to this coin
    coin_blind: Option<Fp>,  // Random blinding factor for coin
    value_blind: Option<Fq>, // Random blinding factor for value commitment
    asset_blind: Option<Fq>, // Random blinding factor for the asset ID
}

impl UtilitiesInstructions<Fp> for MintCircuit {
    type Var = CellValue<Fp>;
}

impl Circuit<Fp> for MintCircuit {
    type Config = Config;
    type FloorPlanner = floor_planner::V1;
    //type FloorPlanner = SimpleFloorPlanner;

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
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
        ];

        let q_add = meta.selector();

        let table_idx = meta.lookup_table_column();

        // let lookup = (
        // table_idx,
        // meta.lookup_table_column(),
        // meta.lookup_table_column(),
        // );

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

        let range_check = LookupRangeCheckConfig::configure(meta, advices[9], table_idx);

        let ecc_config = EccChip::<OrchardFixedBases>::configure(
            meta,
            advices,
            lagrange_coeffs,
            range_check.clone(),
        );

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
            ecc_config,
            poseidon_config,
        }
    }

    fn synthesize(
        &self,
        config: Self::Config,
        mut layouter: impl Layouter<Fp>,
    ) -> Result<(), Error> {
        // Construct the ECC chip.
        let ecc_chip = EccChip::construct(config.ecc_config.clone());

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

        // =============
        // = Coin hash =
        // =============

        // TODO: This is a hack until issue is resolved in poseidon gadget
        let mut coin = Fp::zero();
        let messages = [[pub_x, pub_y], [value, asset], [serial, coin_blind]];
        //let messages = [[pub_x, pub_y], [value, asset]];
        //let messages = [[pub_x, pub_y]];
        for msg in messages.iter() {
            let poseidon_message = layouter.assign_region(
                || "load message",
                |mut region| {
                    let mut message_word = |i: usize| {
                        let val = msg[i].value();
                        let var = region.assign_advice(
                            || format!("load message_{}", i),
                            config.poseidon_config.state()[i],
                            0,
                            || val.ok_or(Error::SynthesisError),
                        )?;
                        region.constrain_equal(var, msg[i].cell())?;
                        Ok(Word::<_, _, OrchardNullifier, 3, 2>::from_inner(
                            StateWord::new(var, val),
                        ))
                    };
                    Ok([message_word(0)?, message_word(1)?])
                },
            )?;

            let poseidon_hasher = PoseidonHash::init(
                PoseidonChip::construct(config.poseidon_config.clone()),
                layouter.namespace(|| "Poseidon init"),
                ConstantLength::<2>,
            )?;

            let poseidon_output =
                poseidon_hasher.hash(layouter.namespace(|| "Poseidon hash"), poseidon_message)?;

            let poseidon_output: CellValue<Fp> = poseidon_output.inner().into();

            if !poseidon_output.value().is_none() {
                coin += poseidon_output.value().unwrap();
            }
        }

        // if coin != Fp::zero() {
        // println!("circuit hash: {:?}", coin);
        // }

        let hash = self.load_private(
            layouter.namespace(|| "load hash"),
            config.advices[0],
            Some(coin),
        )?;

        // Constrain the coin C; index in public values is 0
        layouter.constrain_instance(hash.cell(), config.primary, 0)?;

        // ====================
        // = Value commitment =
        // ====================

        // This constant one is used for multiplication
        let one = self.load_constant(
            layouter.namespace(|| "constant one"),
            config.advices[0],
            Fp::one(),
        )?;

        // v*G_1
        let (commitment, _) = {
            let value_commit_v = OrchardFixedBases::ValueCommitV;
            let value_commit_v = FixedPoint::from_inner(ecc_chip.clone(), value_commit_v);
            value_commit_v.mul_short(layouter.namespace(|| "[value] ValueCommitV"), (value, one))?
        };

        // r_V*G_2
        let (blind, _rcv) = {
            let rcv = self.value_blind;
            let value_commit_r = OrchardFixedBases::ValueCommitR;
            let value_commit_r = FixedPoint::from_inner(ecc_chip.clone(), value_commit_r);
            value_commit_r.mul(layouter.namespace(|| "[value_blind] ValueCommitR"), rcv)?
        };

        // Constrain the x and y; indexes in public values are 1 and 2
        let value_commit = commitment.add(layouter.namespace(|| "valuecommit"), &blind)?;
        layouter.constrain_instance(value_commit.inner().x().cell(), config.primary, 1)?;
        layouter.constrain_instance(value_commit.inner().y().cell(), config.primary, 2)?;

        // ====================
        // = Asset commitment =
        // ====================

        // a*G_1
        let (commitment, _) = {
            let asset_commit_v = OrchardFixedBases::ValueCommitV;
            let asset_commit_v = FixedPoint::from_inner(ecc_chip.clone(), asset_commit_v);
            asset_commit_v.mul_short(layouter.namespace(|| "[asset] ValueCommitV"), (asset, one))?
        };

        // r_A*G_2
        let (blind, _rca) = {
            let rca = self.asset_blind;
            let asset_commit_r = OrchardFixedBases::ValueCommitR;
            let asset_commit_r = FixedPoint::from_inner(ecc_chip.clone(), asset_commit_r);
            asset_commit_r.mul(layouter.namespace(|| "[asset_blind] ValueCommitR"), rca)?
        };

        // Constrain the x and y; indexes in public values are 3 and 4
        let asset_commit = commitment.add(layouter.namespace(|| "assetcommit"), &blind)?;
        layouter.constrain_instance(asset_commit.inner().x().cell(), config.primary, 3)?;
        layouter.constrain_instance(asset_commit.inner().y().cell(), config.primary, 4)?;

        Ok(())
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
        let circuit: MintCircuit = Default::default();

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
        let circuit: MintCircuit = Default::default();

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
    fn create(pk: &ProvingKey, circuits: &[MintCircuit], pubinputs: &[Fp]) -> Result<Self, Error> {
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

    // fn new(bytes: Vec<u8>) -> Self {
    // Proof(bytes)
    // }
}

#[allow(non_snake_case)]
fn pedersen_commitment(value: u64, blind: Fq) -> Ep {
    let hasher = Ep::hash_to_curve(VALUE_COMMITMENT_PERSONALIZATION);
    let V = hasher(&VALUE_COMMITMENT_V_BYTES);
    let R = hasher(&VALUE_COMMITMENT_R_BYTES);
    let value = Fq::from_u64(value);

    V * value + R * blind
}

fn main() {
    let pubkey = Ep::random(&mut OsRng);
    let coords = pubkey.to_affine().coordinates().unwrap();

    let value = 110;
    let asset = 1;

    let value_blind = Fq::random(&mut OsRng);
    let asset_blind = Fq::random(&mut OsRng);

    let serial = Fp::random(&mut OsRng);
    let coin_blind = Fp::random(&mut OsRng);

    let mut coin = Fp::zero();

    let messages = [
        [*coords.x(), *coords.y()],
        [Fp::from(value), Fp::from(asset)],
        [serial, coin_blind],
    ];

    // TODO: This is a hack until issue is fixed in poseidon gadget
    for msg in messages.iter() {
        coin += Hash::init(OrchardNullifier, ConstantLength::<2>).hash(*msg);
    }

    let value_commit = pedersen_commitment(value, value_blind);
    let value_coords = value_commit.to_affine().coordinates().unwrap();

    let asset_commit = pedersen_commitment(asset, asset_blind);
    let asset_coords = asset_commit.to_affine().coordinates().unwrap();

    let mut public_inputs = vec![
        coin,
        *value_coords.x(),
        *value_coords.y(),
        *asset_coords.x(),
        *asset_coords.y(),
    ];

    let circuit = MintCircuit {
        pub_x: Some(*coords.x()),
        pub_y: Some(*coords.y()),
        value: Some(vesta::Scalar::from(value)),
        asset: Some(vesta::Scalar::from(asset)),
        serial: Some(serial),
        coin_blind: Some(coin_blind),
        value_blind: Some(value_blind),
        asset_blind: Some(asset_blind),
    };

    // Valid MockProver
    let prover = MockProver::run(K, &circuit, vec![public_inputs.clone()]).unwrap();
    assert_eq!(prover.verify(), Ok(()));

    // Add 1 to break the public inputs
    public_inputs[0] += Fp::from(0xdeadbeef);

    // Invalid MockProver
    let prover = MockProver::run(K, &circuit, vec![public_inputs.clone()]).unwrap();
    assert!(prover.verify().is_err());

    // Remove 1 to make the public inputs valid again
    public_inputs[0] -= Fp::from(0xdeadbeef);

    // Actual ZK proof
    let start = Instant::now();
    let vk = VerifyingKey::build();
    let pk = ProvingKey::build();
    println!("\nSetup: [{:?}]", start.elapsed());

    let start = Instant::now();
    let proof = Proof::create(&pk, &[circuit], &public_inputs).unwrap();
    println!("Prove: [{:?}]", start.elapsed());

    let start = Instant::now();
    assert!(proof.verify(&vk, &public_inputs).is_ok());
    println!("Verify: [{:?}]", start.elapsed());
}
