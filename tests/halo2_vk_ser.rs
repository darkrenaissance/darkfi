/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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
use std::io::Cursor;

use darkfi_sdk::crypto::{
    pedersen::pedersen_commitment_u64, util::fp_mod_fv, Blind, MerkleNode, MerkleTree, PublicKey,
    SecretKey,
};
use halo2_gadgets::poseidon::{
    primitives as poseidon,
    primitives::{ConstantLength, P128Pow5T3},
};
use halo2_proofs::{
    arithmetic::{CurveAffine, Field},
    circuit::Value,
    pasta::{group::Curve, pallas},
};
use rand::rngs::OsRng;

use darkfi::{
    zk::{
        proof::{ProvingKey, VerifyingKey},
        vm::ZkCircuit,
        vm_heap::{empty_witnesses, Witness},
        Proof,
    },
    zkas::ZkBinary,
    Result,
};

#[test]
fn halo2_vk_ser() -> Result<()> {
    let bincode = include_bytes!("../proof/opcodes.zk.bin");
    let zkbin = ZkBinary::decode(bincode)?;

    let verifier_witnesses = empty_witnesses(&zkbin)?;

    println!("Building vk1");
    let circuit = ZkCircuit::new(verifier_witnesses.clone(), &zkbin);
    let vk1 = VerifyingKey::build(zkbin.k, &circuit);

    println!("Building vk2");
    let circuit = ZkCircuit::new(verifier_witnesses.clone(), &zkbin);
    let vk2 = VerifyingKey::build(zkbin.k, &circuit);

    let mut buf1 = vec![];
    let mut buf2 = vec![];

    println!("Writing vk1");
    vk1.write(&mut buf1)?;

    println!("Writing vk2");
    vk2.write(&mut buf2)?;

    println!("{} kB", buf1.len() / 1024);
    assert_eq!(buf1, buf2);

    println!("Reading vk3");
    let mut buf1_c = Cursor::new(buf1);
    // Construct the circuit to be able to read the VerifyingKey
    let circuit = ZkCircuit::new(empty_witnesses(&zkbin)?, &zkbin);
    let vk3 = VerifyingKey::read::<Cursor<Vec<u8>>, ZkCircuit>(&mut buf1_c, circuit)?;

    println!("Reading vk4");
    let mut buf2_c = Cursor::new(buf2);
    // Construct the circuit to be able to read the VerifyingKey
    let circuit = ZkCircuit::new(empty_witnesses(&zkbin)?, &zkbin);
    let vk4 = VerifyingKey::read::<Cursor<Vec<u8>>, ZkCircuit>(&mut buf2_c, circuit)?;

    // Now let's see if we can verify a proof with all four keys.
    println!("Creating pk");
    let circuit = ZkCircuit::new(verifier_witnesses.clone(), &zkbin);
    let pk = ProvingKey::build(zkbin.k, &circuit);

    let value = 666_u64;
    let value_blind = Blind::random(&mut OsRng);
    let blind = pallas::Base::random(&mut OsRng);
    let secret = pallas::Base::random(&mut OsRng);
    let a = pallas::Base::from(42);
    let b = pallas::Base::from(69);

    let mut tree = MerkleTree::new(100);
    let c0 = pallas::Base::random(&mut OsRng);
    let c1 = pallas::Base::random(&mut OsRng);
    let c3 = pallas::Base::random(&mut OsRng);
    let c2 = {
        let messages = [pallas::Base::one(), pallas::Base::from(2), blind];
        poseidon::Hash::<_, P128Pow5T3, ConstantLength<3>, 3, 2>::init().hash(messages)
    };

    tree.append(MerkleNode::from(c0));
    tree.mark();
    tree.append(MerkleNode::from(c1));
    tree.append(MerkleNode::from(c2));
    let leaf_pos = tree.mark().unwrap();
    tree.append(MerkleNode::from(c3));
    tree.mark();

    let root = tree.root(0).unwrap();
    let merkle_path = tree.witness(leaf_pos, 0).unwrap();
    let leaf_pos: u64 = leaf_pos.into();

    let ephem_secret = SecretKey::random(&mut OsRng);
    let pubkey = PublicKey::from_secret(ephem_secret).inner();
    let (ephem_x, ephem_y) =
        PublicKey::try_from(pubkey * fp_mod_fv(ephem_secret.inner())).unwrap().xy();
    let prover_witnesses = vec![
        Witness::Base(Value::known(pallas::Base::from(value))),
        Witness::Scalar(Value::known(value_blind.inner())),
        Witness::Base(Value::known(blind)),
        Witness::Base(Value::known(a)),
        Witness::Base(Value::known(b)),
        Witness::Base(Value::known(secret)),
        Witness::EcNiPoint(Value::known(pubkey)),
        Witness::Base(Value::known(ephem_secret.inner())),
        Witness::Uint32(Value::known(leaf_pos.try_into().unwrap())),
        Witness::MerklePath(Value::known(merkle_path.try_into().unwrap())),
        Witness::Base(Value::known(pallas::Base::ONE)),
    ];

    let value_commit = pedersen_commitment_u64(value, value_blind);
    let value_coords = value_commit.to_affine().coordinates().unwrap();

    let d_m = [pallas::Base::one(), blind, *value_coords.x(), *value_coords.y()];
    let d = poseidon::Hash::<_, P128Pow5T3, ConstantLength<4>, 3, 2>::init().hash(d_m);

    let public = PublicKey::from_secret(SecretKey::from(secret));
    let (pub_x, pub_y) = public.xy();

    let public_inputs = vec![
        *value_coords.x(),
        *value_coords.y(),
        c2,
        d,
        root.inner(),
        pub_x,
        pub_y,
        ephem_x,
        ephem_y,
        a,
        pallas::Base::ZERO,
    ];

    println!("Creating proof");
    let circuit = ZkCircuit::new(prover_witnesses, &zkbin);
    let proof = Proof::create(&pk, &[circuit], &public_inputs, &mut OsRng)?;

    println!("Verifying with vk1");
    proof.verify(&vk1, &public_inputs)?;

    println!("Verifying with vk2");
    proof.verify(&vk2, &public_inputs)?;

    println!("Verifying with vk3");
    proof.verify(&vk3, &public_inputs)?;

    println!("Verifying with vk4");
    proof.verify(&vk4, &public_inputs)?;

    Ok(())
}
