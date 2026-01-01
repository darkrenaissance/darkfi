/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use std::{
    fs::{File, read_dir},
    io::{Read, Write},
    path::Path,
};

use darkfi::{
    zk::{Proof, ProvingKey, VerifyingKey, Witness, ZkCircuit, empty_witnesses, halo2::Field},
    zkas::ZkBinary,
};
use darkfi_sdk::{
    crypto::{
        MerkleNode, MerkleTree,
        constants::{
            NullifierK,
            OrchardFixedBasesFull::ValueCommitR,
            fixed_bases::{VALUE_COMMITMENT_PERSONALIZATION, VALUE_COMMITMENT_V_BYTES},
        },
        pasta_prelude::{Curve, CurveAffine, CurveExt, Group},
        smt::{EMPTY_NODES_FP, MemoryStorageFp, PoseidonFp, SmtMemoryFp},
        util::{fp_mod_fv, poseidon_hash},
    },
    pasta::{Ep, Fp, Fq, pallas, pallas::Base},
};
use darkfi_serial::serialize;
use halo2_gadgets::ecc::chip::FixedPoint;
use halo2_proofs::circuit::Value;
use rand::rngs::OsRng;

fn main() {
    let entries = read_dir("proof").unwrap();
    // Read each compiled zk file bin
    for entry in entries.flatten() {
        let path = entry.path();
        if !(path.is_file() && path.to_str().unwrap().ends_with(".zk.bin")) {
            continue
        }
        let name = path.file_name().unwrap().to_str().unwrap().split(".").next().unwrap();
        let proof_file = format!("proof/{name}.proof.bin");
        let vk_file = format!("proof/{name}.vks.bin");
        let public_inputs_file = format!("proof/{name}.pi.bin");

        // Skip if already generated
        if Path::new(&proof_file).exists() &&
            Path::new(&vk_file).exists() &&
            Path::new(&public_inputs_file).exists()
        {
            println!("{name} is already generated");
            continue;
        }

        println!("Generating {name} ...");

        // Open zk bin
        let mut file = File::open(&path).unwrap();
        let mut buf = vec![];
        file.read_to_end(&mut buf).unwrap();
        let zkbin = ZkBinary::decode(&buf).unwrap();

        // Get witnesses and public inputs for that particular zk file
        let (witnesses, public_inputs) = retrieve_proof_inputs(name);

        // Generate and save Proof
        let circuit = ZkCircuit::new(witnesses, &zkbin);
        let proving_key = ProvingKey::build(zkbin.k, &circuit);
        let proof = Proof::create(&proving_key, &[circuit], &public_inputs, &mut OsRng).unwrap();

        let proof_export = serialize(&proof);
        let mut f = File::create(&proof_file).unwrap();
        f.write_all(&proof_export).unwrap();

        // Generate and save Verifying Key
        let verifier_witnesses = empty_witnesses(&zkbin).unwrap();
        let circuit = ZkCircuit::new(verifier_witnesses, &zkbin);
        let verifying_key = VerifyingKey::build(zkbin.k, &circuit);

        let mut vk_export = vec![];
        verifying_key.write(&mut vk_export).unwrap();
        let mut f = File::create(&vk_file).unwrap();
        f.write_all(&vk_export).unwrap();

        // Save Public inputs
        let public_inputs_export = serialize(&public_inputs);
        let mut f = File::create(&public_inputs_file).unwrap();
        f.write_all(&public_inputs_export).unwrap();
    }
}

fn retrieve_proof_inputs(name: &str) -> (Vec<Witness>, Vec<Base>) {
    match name {
        "sparse_merkle_root" => {
            let hasher = PoseidonFp::new();
            let store = MemoryStorageFp::new();
            let mut smt = SmtMemoryFp::new(store, hasher.clone(), &EMPTY_NODES_FP);

            let leaves =
                vec![Fp::random(&mut OsRng), Fp::random(&mut OsRng), Fp::random(&mut OsRng)];
            let leaves: Vec<_> = leaves.into_iter().map(|l| (l, l)).collect();
            smt.insert_batch(leaves.clone()).unwrap();

            let (pos, leaf) = leaves[2];

            let root = smt.root();
            let path = smt.prove_membership(&pos);

            let prover_witnesses = vec![
                Witness::SparseMerklePath(Value::known(path.path)),
                Witness::Base(Value::known(leaf)),
            ];

            let public_inputs = vec![root];

            (prover_witnesses, public_inputs)
        }
        "merkle_root" => {
            let mut tree = MerkleTree::new(u32::MAX as usize);
            let node1 = MerkleNode::from(Fp::random(&mut OsRng));
            let node2 = MerkleNode::from(Fp::random(&mut OsRng));
            let node3 = MerkleNode::from(Fp::random(&mut OsRng));
            tree.append(node1);
            tree.mark();
            tree.append(node2);
            let leaf_pos = tree.mark().unwrap();
            tree.append(node3);

            let root = tree.root(0).unwrap().inner();
            let path = tree.witness(leaf_pos, 0).unwrap();

            let prover_witnesses = vec![
                Witness::Base(Value::known(node2.inner())),
                Witness::Uint32(Value::known(u64::from(leaf_pos).try_into().unwrap())),
                Witness::MerklePath(Value::known(path.try_into().unwrap())),
            ];
            let public_inputs = vec![root];

            (prover_witnesses, public_inputs)
        }
        "base_add" => {
            let b1 = Fp::from(4u64);
            let b2 = Fp::from(110u64);
            let prover_witnesses =
                vec![Witness::Base(Value::known(b1)), Witness::Base(Value::known(b2))];
            let public_inputs = vec![b1 + b2];

            (prover_witnesses, public_inputs)
        }
        "base_mul" => {
            let b1 = Fp::from(4u64);
            let b2 = Fp::from(110u64);
            let prover_witnesses =
                vec![Witness::Base(Value::known(b1)), Witness::Base(Value::known(b2))];
            let public_inputs = vec![b1 * b2];

            (prover_witnesses, public_inputs)
        }
        "base_sub" => {
            let b1 = Fp::from(4u64);
            let b2 = Fp::from(110u64);
            let prover_witnesses =
                vec![Witness::Base(Value::known(b1)), Witness::Base(Value::known(b2))];
            let public_inputs = vec![b1 - b2];

            (prover_witnesses, public_inputs)
        }
        "ec_add" => {
            let p1 = Ep::random(&mut OsRng);
            let p2 = Ep::random(&mut OsRng);
            let sum = (p1 + p2).to_affine();
            let sum_x = *sum.coordinates().unwrap().x();
            let sum_y = *sum.coordinates().unwrap().y();
            let prover_witnesses =
                vec![Witness::EcPoint(Value::known(p1)), Witness::EcPoint(Value::known(p2))];
            let public_inputs = vec![sum_x, sum_y];

            (prover_witnesses, public_inputs)
        }
        "ec_mul" => {
            let scalar_blind = Fq::random(&mut OsRng);
            let vcr = (ValueCommitR.generator() * scalar_blind).to_affine();
            let vcr_x = *vcr.coordinates().unwrap().x();
            let vcr_y = *vcr.coordinates().unwrap().y();
            let prover_witnesses = vec![Witness::Scalar(Value::known(scalar_blind))];
            let public_inputs = vec![vcr_x, vcr_y];

            (prover_witnesses, public_inputs)
        }
        "ec_mul_base" => {
            let secret_key = Fp::random(&mut OsRng);
            let pubkey = (NullifierK.generator() * fp_mod_fv(secret_key)).to_affine();
            let pubkey_x = *pubkey.coordinates().unwrap().x();
            let pubkey_y = *pubkey.coordinates().unwrap().y();
            let prover_witnesses = vec![Witness::Base(Value::known(secret_key))];
            let public_inputs = vec![pubkey_x, pubkey_y];

            (prover_witnesses, public_inputs)
        }
        "ec_mul_short" => {
            // we can't use Fp::random() since it can be more than u64::MAX and we need value to be u64
            let value = Fp::from(42);
            let hasher = pallas::Point::hash_to_curve(VALUE_COMMITMENT_PERSONALIZATION);
            let val_commit = hasher(&VALUE_COMMITMENT_V_BYTES);

            let vcv = (val_commit * fp_mod_fv(value)).to_affine();
            let vcv_x = *vcv.coordinates().unwrap().x();
            let vcv_y = *vcv.coordinates().unwrap().y();
            let prover_witnesses = vec![Witness::Base(Value::known(value))];
            let public_inputs = vec![vcv_x, vcv_y];

            (prover_witnesses, public_inputs)
        }
        "ec_mul_var_base" => {
            let ephem_secret = Fp::random(&mut OsRng);
            let pubkey = NullifierK.generator() * fp_mod_fv(ephem_secret);
            let ephem_pub = (pubkey * fp_mod_fv(ephem_secret)).to_affine();
            let ephem_pub_x = *ephem_pub.coordinates().unwrap().x();
            let ephem_pub_y = *ephem_pub.coordinates().unwrap().y();
            let prover_witnesses = vec![
                Witness::Base(Value::known(ephem_secret)),
                Witness::EcNiPoint(Value::known(pubkey)),
            ];
            let public_inputs = vec![ephem_pub_x, ephem_pub_y];

            (prover_witnesses, public_inputs)
        }
        "ec_get_x" => {
            let p = Ep::random(&mut OsRng);
            let x = *p.to_affine().coordinates().unwrap().x();
            let prover_witnesses = vec![Witness::EcPoint(Value::known(p))];
            let public_inputs = vec![x];

            (prover_witnesses, public_inputs)
        }
        "ec_get_y" => {
            let p = Ep::random(&mut OsRng);
            let y = *p.to_affine().coordinates().unwrap().y();
            let prover_witnesses = vec![Witness::EcPoint(Value::known(p))];
            let public_inputs = vec![y];

            (prover_witnesses, public_inputs)
        }
        "poseidon_hash" => {
            let a = Fp::random(&mut OsRng);
            let b = Fp::random(&mut OsRng);
            let hash = poseidon_hash([a, b]);
            let prover_witnesses =
                vec![Witness::Base(Value::known(a)), Witness::Base(Value::known(b))];
            let public_inputs = vec![hash];

            (prover_witnesses, public_inputs)
        }
        "constrain_instance" => {
            let a = Fp::random(&mut OsRng);
            let prover_witnesses = vec![Witness::Base(Value::known(a))];
            let public_inputs = vec![a];

            (prover_witnesses, public_inputs)
        }
        "witness_base" => (vec![], vec![Fp::from(2)]),
        "constrain_equal_base" => {
            let a = Fp::from(23);
            let prover_witnesses =
                vec![Witness::Base(Value::known(a)), Witness::Base(Value::known(a))];

            (prover_witnesses, vec![])
        }
        "constrain_equal_point" => {
            let a = Ep::random(&mut OsRng);
            let prover_witnesses =
                vec![Witness::EcPoint(Value::known(a)), Witness::EcPoint(Value::known(a))];

            (prover_witnesses, vec![])
        }
        "less_than_strict" => {
            let a = Fp::from(23);
            let b = Fp::from(42);
            let prover_witnesses =
                vec![Witness::Base(Value::known(a)), Witness::Base(Value::known(b))];

            (prover_witnesses, vec![])
        }
        "less_than_loose" => {
            let a = Fp::from(23);
            let prover_witnesses =
                vec![Witness::Base(Value::known(a)), Witness::Base(Value::known(a))];

            (prover_witnesses, vec![])
        }
        "bool_check" => {
            let a = Fp::from(1);
            let prover_witnesses = vec![Witness::Base(Value::known(a))];

            (prover_witnesses, vec![])
        }
        "cond_select" => {
            let a = Fp::from(23);
            let b = Fp::from(42);
            let cond = Fp::from(1);
            let prover_witnesses = vec![
                Witness::Base(Value::known(a)),
                Witness::Base(Value::known(b)),
                Witness::Base(Value::known(cond)),
            ];
            let public_inputs = vec![a];

            (prover_witnesses, public_inputs)
        }
        "zero_cond" => {
            let a = Fp::from(0);
            let b = Fp::from(23);
            let prover_witnesses =
                vec![Witness::Base(Value::known(a)), Witness::Base(Value::known(b))];
            let public_inputs = vec![a];

            (prover_witnesses, public_inputs)
        }
        "range_check" => {
            let a = Fp::from(23);
            let prover_witnesses = vec![Witness::Base(Value::known(a))];

            (prover_witnesses, vec![])
        }
        "debug" => {
            let a = Fp::from(23);
            let prover_witnesses = vec![Witness::Base(Value::known(a))];

            (prover_witnesses, vec![])
        }
        _ => panic!("unsupported Zk script"),
    }
}
