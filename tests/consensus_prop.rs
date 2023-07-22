/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

use darkfi::{
    zk::{empty_witnesses, halo2::Value, Proof, ProvingKey, VerifyingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{
        pasta_prelude::{Curve, CurveAffine, Field},
        pedersen_commitment_u64, poseidon_hash, MerkleNode, MerkleTree, PublicKey, SecretKey,
    },
    pasta::pallas,
};
use halo2_proofs::dev::MockProver;
use log::info;
use rand::rngs::OsRng;

pub const SECRET_KEY_PREFIX: pallas::Base = pallas::Base::from_raw([4, 0, 0, 0]);
pub const SEED_PREFIX: pallas::Base = pallas::Base::from_raw([3, 0, 0, 0]);
pub const SERIAL_PREFIX: pallas::Base = pallas::Base::from_raw([2, 0, 0, 0]);
pub const MU_Y_PREFIX: pallas::Base = pallas::Base::from_raw([22, 0, 0, 0]);
pub const MU_RHO_PREFIX: pallas::Base = pallas::Base::from_raw([5, 0, 0, 0]);
pub const HEADSTART: pallas::Base = pallas::Base::from_raw([
    11731824086999220879,
    11830614503713258191,
    737869762948382064,
    46116860184273879,
]);

#[test]
fn consensus_prop() -> Result<()> {
    simplelog::TermLogger::init(
        simplelog::LevelFilter::Info,
        //simplelog::LevelFilter::Trace,
        simplelog::ConfigBuilder::new().build(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )
    .unwrap();

    let input_serial = pallas::Base::from(pallas::Base::from(10));
    //let input_serial = pallas::Base::from(pallas::Base::from(2));

    let mut merkle_tree = MerkleTree::new(1);

    let input_secret_key = SecretKey::from(pallas::Base::from(42));
    let input_value = 100_000_000_000;
    let epoch = pallas::Base::from(0);
    let reward = 100_000_000;
    let input_value_blind = pallas::Scalar::random(&mut OsRng);
    let output_reward_blind = pallas::Scalar::random(&mut OsRng);
    let output_value_blind = input_value_blind + output_reward_blind;
    let output_value = input_value + reward;

    let (input_x, input_y) = PublicKey::from_secret(input_secret_key).xy();
    let input_coin = poseidon_hash([input_x, input_y, input_value.into(), epoch, input_serial]);

    assert!(merkle_tree.append(MerkleNode::from(input_coin)));
    let input_leaf_pos = merkle_tree.mark().unwrap();
    let merkle_path = merkle_tree.witness(input_leaf_pos, 0).unwrap();

    // Public inputs
    let nullifier = poseidon_hash([input_secret_key.inner(), input_serial]);
    let root = merkle_tree.root(0).unwrap();
    let input_value_commit = pedersen_commitment_u64(input_value, input_value_blind);
    let output_value_commit = pedersen_commitment_u64(output_value, output_value_blind);
    let output_secret_key =
        SecretKey::from(poseidon_hash([SECRET_KEY_PREFIX, input_secret_key.inner()]));
    let output_serial = poseidon_hash([SERIAL_PREFIX, input_secret_key.inner(), input_serial]);
    let (output_x, output_y) = PublicKey::from_secret(output_secret_key).xy();
    let output_coin =
        poseidon_hash([output_x, output_y, output_value.into(), pallas::Base::ZERO, output_serial]);

    let mu_y = pallas::Base::from(1);
    let mu_rho = pallas::Base::from(1);
    let seed = poseidon_hash([SEED_PREFIX, input_serial]);
    let y = poseidon_hash([seed, mu_y]);
    let rho = poseidon_hash([seed, mu_rho]);

    let sigma1 = pallas::Base::from(1);
    let sigma2 = pallas::Base::from(1);
    let value = pallas::Base::from(input_value);
    let shifted_target = sigma1 * value + sigma2 * value * value + HEADSTART;

    info!("y = {:?}", y);
    info!("T = {:?}", shifted_target);
    info!("y < T == {}", y < shifted_target);

    let zkbin = include_bytes!("../src/contract/consensus/proof/consensus_proposal_v1.zk.bin");
    let zkbin = ZkBinary::decode(&zkbin[..])?;

    let prover_witnesses = vec![
        Witness::Base(Value::known(input_secret_key.inner())),
        Witness::Base(Value::known(input_serial)),
        Witness::Base(Value::known(pallas::Base::from(input_value))),
        Witness::Base(Value::known(epoch)),
        Witness::Base(Value::known(pallas::Base::from(reward))),
        Witness::Scalar(Value::known(input_value_blind)),
        Witness::Uint32(Value::known(u64::from(input_leaf_pos).try_into().unwrap())),
        Witness::MerklePath(Value::known(merkle_path.try_into().unwrap())),
        Witness::Scalar(Value::known(output_value_blind)),
        Witness::Base(Value::known(mu_y)),
        Witness::Base(Value::known(mu_rho)),
        Witness::Base(Value::known(sigma1)),
        Witness::Base(Value::known(sigma2)),
        Witness::Base(Value::known(HEADSTART)),
    ];

    let input_value_coords = input_value_commit.to_affine().coordinates().unwrap();
    let output_value_coords = output_value_commit.to_affine().coordinates().unwrap();

    let public_inputs = vec![
        nullifier,
        epoch,
        input_x,
        input_y,
        root.inner(),
        *input_value_coords.x(),
        *input_value_coords.y(),
        pallas::Base::from(reward),
        *output_value_coords.x(),
        *output_value_coords.y(),
        output_coin,
        mu_y,
        y,
        mu_rho,
        rho,
        sigma1,
        sigma2,
        HEADSTART,
    ];

    let prover_circuit = ZkCircuit::new(prover_witnesses, &zkbin);
    let mockprover = MockProver::run(zkbin.k, &prover_circuit, vec![public_inputs.clone()])?;
    mockprover.assert_satisfied();

    let verifier_witnesses = empty_witnesses(&zkbin);
    let circuit = ZkCircuit::new(verifier_witnesses, &zkbin);

    let proving_key = ProvingKey::build(zkbin.k, &circuit);
    let verifying_key = VerifyingKey::build(zkbin.k, &circuit);

    let proof = Proof::create(&proving_key, &[prover_circuit], &public_inputs, &mut OsRng)?;
    proof.verify(&verifying_key, &public_inputs)?;

    Ok(())
}
