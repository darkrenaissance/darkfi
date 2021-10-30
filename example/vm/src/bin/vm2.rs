use group::{Curve, Group};
use halo2::{
    arithmetic::{CurveAffine, CurveExt, Field, FieldExt},
    circuit::{Layouter, SimpleFloorPlanner},
    dev::MockProver,
    pasta::{pallas, vesta},
    plonk,
    poly::commitment,
    transcript::{Blake2bRead, Blake2bWrite},
};
use halo2_poseidon::{
    gadget::{Hash as PoseidonHash, Word},
    pow5t3::{Pow5T3Chip as PoseidonChip, StateWord},
    primitive::{ConstantLength, Hash, P128Pow5T3 as OrchardNullifier},
};
use orchard::constants::fixed_bases::{
    OrchardFixedBases, VALUE_COMMITMENT_PERSONALIZATION, VALUE_COMMITMENT_R_BYTES,
    VALUE_COMMITMENT_V_BYTES,
};
use rand::rngs::OsRng;
use std::{collections::HashMap, fs::File, time::Instant};

use drk::serial::Decodable;

// The number of rows in our circuit cannot exceed 2^k
const K: u32 = 9;

#[allow(non_snake_case)]
pub fn pedersen_commitment(value: u64, blind: pallas::Scalar) -> pallas::Point {
    let hasher = pallas::Point::hash_to_curve(VALUE_COMMITMENT_PERSONALIZATION);
    let V = hasher(&VALUE_COMMITMENT_V_BYTES);
    let R = hasher(&VALUE_COMMITMENT_R_BYTES);
    let value = pallas::Scalar::from_u64(value);

    V * value + R * blind
}

#[derive(Debug)]
struct VerifyingKey {
    params: commitment::Params<vesta::Affine>,
    vk: plonk::VerifyingKey<vesta::Affine>,
}

impl VerifyingKey {
    fn build(empty_circuit: drk::vm2::ZkCircuit) -> Self {
        let params = commitment::Params::new(K);

        let vk = plonk::keygen_vk(&params, &empty_circuit).unwrap();

        VerifyingKey { params, vk }
    }
}

#[derive(Debug)]
struct ProvingKey {
    params: commitment::Params<vesta::Affine>,
    pk: plonk::ProvingKey<vesta::Affine>,
}

impl ProvingKey {
    fn build(empty_circuit: drk::vm2::ZkCircuit) -> Self {
        let params = commitment::Params::new(K);

        let vk = plonk::keygen_vk(&params, &empty_circuit).unwrap();
        let pk = plonk::keygen_pk(&params, vk, &empty_circuit).unwrap();

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
    fn create(
        pk: &ProvingKey,
        circuits: &[drk::vm2::ZkCircuit],
        pubinputs: &[pallas::Base],
    ) -> Result<Self, plonk::Error> {
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

    fn verify(&self, vk: &VerifyingKey, pubinputs: &[pallas::Base]) -> Result<(), plonk::Error> {
        let msm = vk.params.empty_msm();
        let mut transcript = Blake2bRead::init(&self.0[..]);
        let guard = plonk::verify_proof(&vk.params, &vk.vk, msm, &[&[pubinputs]], &mut transcript)?;
        let msm = guard.clone().use_challenges();
        if msm.eval() {
            Ok(())
        } else {
            Err(plonk::Error::ConstraintSystemFailure)
        }
    }

    // fn new(bytes: Vec<u8>) -> Self {
    // Proof(bytes)
    // }
}

fn main() -> std::result::Result<(), failure::Error> {
    let start = Instant::now();
    let file = File::open("../../proof/mint.zk.bin")?;
    let zkbin = drk::vm2::ZkBinary::decode(file)?;
    for contract_name in zkbin.contracts.keys() {
        println!("Loaded '{}' contract.", contract_name);
    }
    println!("Load time: [{:?}]", start.elapsed());

    let contract = &zkbin.contracts["Mint"];

    //contract.witness_base(...);
    //contract.witness_base(...);
    //contract.witness_base(...);

    let pubkey = pallas::Point::random(&mut OsRng);
    let coords = pubkey.to_affine().coordinates().unwrap();

    let value = 110;
    let asset = 1;

    let value_blind = pallas::Scalar::random(&mut OsRng);
    let asset_blind = pallas::Scalar::random(&mut OsRng);

    let serial = pallas::Base::random(&mut OsRng);
    let coin_blind = pallas::Base::random(&mut OsRng);

    let mut coin = pallas::Base::zero();

    let messages = [
        [*coords.x(), *coords.y()],
        [pallas::Base::from(value), pallas::Base::from(asset)],
        [serial, coin_blind],
    ];

    for msg in messages.iter() {
        coin += Hash::init(OrchardNullifier, ConstantLength::<2>).hash(*msg);
    }

    let coin2 = Hash::init(OrchardNullifier, ConstantLength::<2>).hash([*coords.x(), *coords.y()]);

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

    let mut const_fixed_points = HashMap::new();
    const_fixed_points.insert(
        "VALUE_COMMIT_VALUE".to_string(),
        OrchardFixedBases::ValueCommitV,
    );
    const_fixed_points.insert(
        "VALUE_COMMIT_RANDOM".to_string(),
        OrchardFixedBases::ValueCommitR,
    );

    let mut circuit = drk::vm2::ZkCircuit::new(const_fixed_points, &zkbin.constants, contract);
    let empty_circuit = circuit.clone();

    circuit.witness_base("pub_x", *coords.x())?;
    circuit.witness_base("pub_y", *coords.y())?;
    circuit.witness_base("value", pallas::Base::from(value))?;
    circuit.witness_base("asset", pallas::Base::from(asset))?;
    circuit.witness_base("serial", serial)?;
    circuit.witness_base("coin_blind", coin_blind)?;
    circuit.witness_scalar("value_blind", value_blind)?;
    circuit.witness_scalar("asset_blind", asset_blind)?;

    // Valid MockProver
    let prover = MockProver::run(K, &circuit, vec![public_inputs.clone()]).unwrap();
    assert_eq!(prover.verify(), Ok(()));

    // Actual ZK proof
    let start = Instant::now();
    let vk = VerifyingKey::build(empty_circuit.clone());
    let pk = ProvingKey::build(empty_circuit.clone());
    println!("\nSetup: [{:?}]", start.elapsed());

    let start = Instant::now();
    let proof = Proof::create(&pk, &[circuit], &public_inputs).unwrap();
    println!("Prove: [{:?}]", start.elapsed());

    let start = Instant::now();
    assert!(proof.verify(&vk, &public_inputs).is_ok());
    println!("Verify: [{:?}]", start.elapsed());

    Ok(())
}
