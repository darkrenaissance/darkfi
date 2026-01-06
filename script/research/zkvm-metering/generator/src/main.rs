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
    fs::{read_dir, File},
    io::{Read, Write},
    path::Path,
};

use darkfi::{
    zk::{empty_witnesses, Proof, ProvingKey, VerifyingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
};
use darkfi_sdk::pasta::{pallas::Base, Eq, Fp};
use darkfi_serial::serialize;
use halo2_proofs::dev::CircuitCost;
use rand::rngs::OsRng;

/// Dao contract zk proofs witness and public input generator.
mod dao;

/// Money contract zk proofs witness and public input generator.
mod money;

/// Opcode zk proofs witness and public input generator.
mod opcodes;

fn main() {
    // Read all src/*/proof directories contents
    let mut zk_bin_files = vec![];

    for entry in read_dir("src").unwrap() {
        let path = entry.unwrap().path();
        if !path.is_dir() {
            continue;
        }
        let path = path.join("proof");
        if path.exists() && path.is_dir() {
            read_dir(path).unwrap().flatten().for_each(|e| {
                let zk_path = e.path();
                if zk_path.is_file() && zk_path.to_str().unwrap().ends_with(".zk.bin") {
                    zk_bin_files.push(zk_path)
                }
            })
        }
    }

    // Read each compiled zk file bin
    for path in zk_bin_files {
        let base_dir = path.parent().unwrap().to_str().unwrap();
        let name = path.file_name().unwrap().to_str().unwrap().split(".").next().unwrap();
        let proof_file = format!("{base_dir}/{name}.proof.bin");
        let vk_file = format!("{base_dir}/{name}.vks.bin");
        let public_inputs_file = format!("{base_dir}/{name}.pi.bin");

        // Skip if already generated
        if Path::new(&proof_file).exists() &&
            Path::new(&vk_file).exists() &&
            Path::new(&public_inputs_file).exists()
        {
            println!("{name} is already generated");
            continue;
        }

        println!("Generating {name}....");

        // Open zk bin
        let mut file = File::open(&path).unwrap();
        let mut buf = vec![];
        file.read_to_end(&mut buf).unwrap();
        let zkbin = ZkBinary::decode(&buf, false).unwrap();

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

        // Show circuit cost
        let circuit_cost = CircuitCost::<Eq, ZkCircuit>::measure(zkbin.k, &circuit);
        println!("{:#?}", circuit_cost);
    }
}

fn retrieve_proof_inputs(name: &str) -> (Vec<Witness>, Vec<Base>) {
    match name {
        "sparse_merkle_root" => opcodes::sparse_merkle_root(),
        "merkle_root" => opcodes::merkle_root(),
        "base_add" => opcodes::base_add(),
        "base_mul" => opcodes::base_mul(),
        "base_sub" => opcodes::base_sub(),
        "ec_add" => opcodes::ec_add(),
        "ec_mul" => opcodes::ec_mul(),
        "ec_mul_base" => opcodes::ec_mul_base(),
        "ec_mul_short" => opcodes::ec_mul_short(),
        "ec_mul_var_base" => opcodes::ec_mul_var_base(),
        "ec_get_x" => opcodes::ec_get_x(),
        "ec_get_y" => opcodes::ec_get_y(),
        "poseidon_hash" => opcodes::poseidon_hash_opcode(),
        "constrain_instance" => opcodes::constrain_instance(),
        "witness_base" => (vec![], vec![Fp::from(2)]),
        "constrain_equal_base" => opcodes::constrain_equal_base(),
        "constrain_equal_point" => opcodes::constrain_equal_point(),
        "less_than_strict" => opcodes::less_than_strict(),
        "less_than_loose" => opcodes::less_than_loose(),
        "bool_check" => opcodes::bool_check(),
        "cond_select" => opcodes::cond_select(),
        "zero_cond" => opcodes::zero_cond(),
        "range_check" => opcodes::range_check(),
        "debug" => opcodes::debug(),
        "money_mint_v1" => money::mint_v1(),
        "money_burn_v1" => money::burn_v1(),
        "money_fee_v1" => money::fee_v1(),
        "money_token_mint_v1" => money::token_mint_v1(),
        "money_auth_token_mint_v1" => money::auth_token_mint_v1(),
        "dao_mint" => dao::mint(),
        "dao_propose_input" => dao::propose_input(),
        "dao_propose_main" => dao::propose_main(),
        "dao_vote_input" => dao::vote_input(),
        "dao_vote_main" => dao::vote_main(),
        "dao_exec" => dao::exec(),
        "dao_early_exec" => dao::early_exec(),
        "dao_auth_money_transfer" => dao::auth_money_transfer(),
        "dao_auth_money_transfer_enc_coin" => dao::auth_money_transfer_enc_coin(),
        _ => panic!("unsupported Zk script"),
    }
}
