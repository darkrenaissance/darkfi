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

use std::time::Instant;

use darkfi::{
    zk::{empty_witnesses, halo2::Value, Proof, ProvingKey, VerifyingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
};
use darkfi_sdk::{
    crypto::{
        pasta_prelude::Field,
        poseidon_hash,
        smt::{MemoryStorageFp, PoseidonFp, SmtMemoryFp, EMPTY_NODES_FP},
    },
    pasta::{group::ff::FromUniformBytes, pallas},
};
use rand::rngs::OsRng;

struct Identity {
    identity_nullifier: pallas::Base,
    identity_trapdoor: pallas::Base,
    user_message_limit: pallas::Base,
}

impl Identity {
    fn new(user_message_limit: pallas::Base) -> Self {
        Self {
            identity_nullifier: pallas::Base::random(&mut OsRng),
            identity_trapdoor: pallas::Base::random(&mut OsRng),
            user_message_limit,
        }
    }

    fn commitment(&self) -> pallas::Base {
        let identity_secret = poseidon_hash([self.identity_nullifier, self.identity_trapdoor]);
        let identity_secret_hash = poseidon_hash([identity_secret, self.user_message_limit]);

        poseidon_hash([identity_secret_hash])
    }
}

/// Hash message modulo Fp
/// In DarkIRC/eventgraph this could be the event ID
fn hash_message(msg: &str) -> pallas::Base {
    let message_hash = blake3::hash(msg.as_bytes());

    let mut buf = [0u8; 64];
    buf[..blake3::OUT_LEN].copy_from_slice(message_hash.as_bytes());
    pallas::Base::from_uniform_bytes(&buf)
}

fn main() {
    // There exists a Sparse Merkle Tree of identity commitments that
    // serves as the user registry. If a leaf is NULL, it should mean
    // that the identity is non-existent and thus should not be accepted.
    let hasher = PoseidonFp::new();
    let store = MemoryStorageFp::new();
    let mut identity_tree = SmtMemoryFp::new(store, hasher.clone(), &EMPTY_NODES_FP);

    // Per-app identifier
    let rln_identifier = pallas::Base::from(42);

    // Create three accounts
    let id0 = Identity::new(pallas::Base::from(5));
    let id1 = Identity::new(pallas::Base::from(2));
    let id2 = Identity::new(pallas::Base::from(1));

    // ============
    // Registration
    // ============
    let register_zkbin = include_bytes!("../register.zk.bin");
    let register_zkbin = ZkBinary::decode(register_zkbin).unwrap();
    let register_empty_circuit =
        ZkCircuit::new(empty_witnesses(&register_zkbin).unwrap(), &register_zkbin);

    print!("[Register] Building Proving key... ");
    let now = Instant::now();
    let register_pk = ProvingKey::build(register_zkbin.k, &register_empty_circuit);
    println!("[{:?}]", now.elapsed());

    print!("[Register] Building Verifying key... ");
    let now = Instant::now();
    let register_vk = VerifyingKey::build(register_zkbin.k, &register_empty_circuit);
    println!("[{:?}]", now.elapsed());

    for (i, id) in [id0, id1, id2].iter().enumerate() {
        // Create ZK proof
        // This 6 message limit is arbitrary and should likely be based on stake.
        let witnesses = vec![
            Witness::Base(Value::known(id.identity_nullifier)),
            Witness::Base(Value::known(id.identity_trapdoor)),
            Witness::Base(Value::known(id.user_message_limit)),
            Witness::Base(Value::known(pallas::Base::from(6))),
        ];
        let public_inputs = vec![id.commitment(), pallas::Base::from(6)];

        print!("[Register] Creating ZK proof for id{i}... ");
        let now = Instant::now();
        let register_circuit = ZkCircuit::new(witnesses, &register_zkbin);
        let proof =
            Proof::create(&register_pk, &[register_circuit], &public_inputs, &mut OsRng).unwrap();
        println!("[{:?}]", now.elapsed());

        // Verify ZK proof
        print!("[Register] Verifying ZK proof for id{i}... ");
        assert!(proof.verify(&register_vk, &public_inputs).is_ok());
        println!("[{:?}]", now.elapsed());

        let leaf = vec![id.commitment()];
        let leaf: Vec<_> = leaf.into_iter().map(|l| (l, l)).collect();
        // TODO: Should verify that identity doesn't exist yet before insert.
        identity_tree.insert_batch(leaf.clone()).unwrap(); // leaf == pos
        assert_eq!(leaf[0].0, id.commitment());
        assert_eq!(leaf[0].1, id.commitment());
    }

    // At this point we have 3 identities registered. They also all have
    // different message limits per epoch.
}
