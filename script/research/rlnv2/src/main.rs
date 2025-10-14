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
    collections::BTreeMap,
    time::{Instant, UNIX_EPOCH},
};

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

#[derive(Copy, Clone)]
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

#[derive(Debug, Clone)]
struct ShareData {
    pub x_shares: Vec<pallas::Base>,
    pub y_shares: Vec<pallas::Base>,
}

impl ShareData {
    fn new() -> Self {
        Self { x_shares: vec![], y_shares: vec![] }
    }
}

#[derive(Debug, Default)]
struct MessageMetadata {
    data: BTreeMap<pallas::Base, BTreeMap<pallas::Base, ShareData>>,
}

impl MessageMetadata {
    fn new() -> Self {
        Self { data: BTreeMap::new() }
    }

    fn add_share(
        &mut self,
        external_nullifier: pallas::Base,
        internal_nullifier: pallas::Base,
        x: pallas::Base,
        y: pallas::Base,
    ) {
        let inner_map = self.data.entry(external_nullifier).or_insert_with(BTreeMap::new);
        let share_data = inner_map.entry(internal_nullifier).or_insert_with(ShareData::new);

        share_data.x_shares.push(x);
        share_data.y_shares.push(y);
    }

    fn get_shares(
        &self,
        external_nullifier: &pallas::Base,
        internal_nullifier: &pallas::Base,
    ) -> Vec<(pallas::Base, pallas::Base)> {
        if let Some(inner_map) = self.data.get(external_nullifier) {
            if let Some(share_data) = inner_map.get(internal_nullifier) {
                return share_data
                    .x_shares
                    .iter()
                    .cloned()
                    .zip(share_data.y_shares.iter().cloned())
                    .collect()
            }
        }

        vec![]
    }

    fn is_duplicate(
        &self,
        external_nullifier: &pallas::Base,
        internal_nullifier: &pallas::Base,
        x: &pallas::Base,
        y: &pallas::Base,
    ) -> bool {
        if let Some(inner_map) = self.data.get(external_nullifier) {
            if let Some(share_data) = inner_map.get(internal_nullifier) {
                return share_data.x_shares.contains(x) && share_data.y_shares.contains(y);
            }
        }

        false
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

fn sss_recover(shares: &[(pallas::Base, pallas::Base)]) -> pallas::Base {
    let mut secret = pallas::Base::zero();
    for (j, share_j) in shares.iter().enumerate() {
        let mut prod = pallas::Base::one();
        for (i, share_i) in shares.iter().enumerate() {
            if i != j {
                prod *= share_i.0 * (share_i.0 - share_j.0).invert().unwrap();
            }
        }

        prod *= share_j.1;
        secret += prod;
    }

    secret
}

fn main() {
    // There exists a Sparse Merkle Tree of identity commitments that
    // serves as the user registry. If a leaf is NULL, it should mean
    // that the identity is non-existent and thus should not be accepted.
    let hasher = PoseidonFp::new();
    let store = MemoryStorageFp::new();
    let mut identity_tree = SmtMemoryFp::new(store, hasher.clone(), &EMPTY_NODES_FP);

    // Per-app identifier
    let rln_identifier = pallas::Base::from(1000);

    // Create two accounts
    let id0 = Identity::new(pallas::Base::from(2));
    let id1 = Identity::new(pallas::Base::from(1));

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

    for (i, id) in [id0, id1].iter().enumerate() {
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
        let now = Instant::now();
        assert!(proof.verify(&register_vk, &public_inputs).is_ok());
        println!("[{:?}]", now.elapsed());

        let leaf = vec![id.commitment()];
        let leaf: Vec<_> = leaf.into_iter().map(|l| (l, l)).collect();
        // TODO: Recipients should verify that identity doesn't exist already before insert.
        identity_tree.insert_batch(leaf.clone()).unwrap(); // leaf == pos
        assert_eq!(leaf[0].0, id.commitment());
        assert_eq!(leaf[0].1, id.commitment());
    }

    // At this point we have 2 identities registered.

    // ==========
    // Signalling
    // ==========
    let signal_zkbin = include_bytes!("../signal.zk.bin");
    let signal_zkbin = ZkBinary::decode(signal_zkbin).unwrap();
    let signal_empty_circuit =
        ZkCircuit::new(empty_witnesses(&signal_zkbin).unwrap(), &signal_zkbin);

    print!("[Signal] Building Proving key... ");
    let now = Instant::now();
    let signal_pk = ProvingKey::build(signal_zkbin.k, &signal_empty_circuit);
    println!("[{:?}]", now.elapsed());

    print!("[Signal] Building Verifying key... ");
    let now = Instant::now();
    let signal_vk = VerifyingKey::build(signal_zkbin.k, &signal_empty_circuit);
    println!("[{:?}]", now.elapsed());

    // Our epoch length will be 10s. This normally means one message every
    // 10 seconds. In RLNv2-DIFF we have N messages every epoch.
    // This works because of integer division, so for example:
    // 1697472000, 1697472005, and 1697472009 are the same epoch, but
    // 1697472010 would be the new epoch.
    // In practice, if our client realizes we're sending too fast, we could
    // also queue it.
    let epoch_len = 10_u64;

    // =========================
    // Account 0 sends a message
    // =========================

    // 1. Construct share
    let epoch = pallas::Base::from(UNIX_EPOCH.elapsed().unwrap().as_secs() as u64 / epoch_len);
    let message_id = pallas::Base::from(0); // This should increment each msg

    let external_nullifier = poseidon_hash([epoch, rln_identifier]);
    let a_0 = poseidon_hash([id0.identity_nullifier, id0.identity_trapdoor]);
    let a_1 = poseidon_hash([a_0, external_nullifier, message_id]);
    let x = hash_message("hello");
    let y = a_0 + x * a_1;

    let internal_nullifier = poseidon_hash([a_1]);

    // 2. Inclusion proof
    let root = identity_tree.root();
    let path = identity_tree.prove_membership(&id0.commitment());
    assert!(path.verify(&root, &id0.commitment(), &id0.commitment()));

    // 3. ZK proof
    let witnesses = vec![
        Witness::Base(Value::known(id0.identity_nullifier)),
        Witness::Base(Value::known(id0.identity_trapdoor)),
        Witness::Base(Value::known(id0.user_message_limit)),
        Witness::SparseMerklePath(Value::known(path.path)),
        Witness::Base(Value::known(x)),
        Witness::Base(Value::known(message_id)),
        Witness::Base(Value::known(epoch)),
    ];

    let public_inputs = vec![root, external_nullifier, x, y, internal_nullifier];

    print!("[Signal] Creating ZK proof for 0:0... ");
    let now = Instant::now();
    let signal_circuit = ZkCircuit::new(witnesses, &signal_zkbin);
    let proof = Proof::create(&signal_pk, &[signal_circuit], &public_inputs, &mut OsRng).unwrap();
    print!("[{:?}] ", now.elapsed());
    println!("({} bytes)", proof.as_ref().len());

    // ============
    // Verification
    // ============
    print!("[Signal] Verifying ZK proof for 0:0... ");
    let now = Instant::now();
    assert!(proof.verify(&signal_vk, &public_inputs).is_ok());
    println!("[{:?}]", now.elapsed());

    // Each user of the protocol must store metadata for each message
    // received by each user, for the given epoch. The data can be
    // deleted when the epoch passes.
    let mut metadata = MessageMetadata::new();
    if metadata.is_duplicate(&external_nullifier, &internal_nullifier, &x, &y) {
        println!("[Signal] Duplicate Message!");
        return
    }

    // Add share
    metadata.add_share(external_nullifier, internal_nullifier, x, y);

    // Now let's try to send another message in the same epoch.
    // id0 has a limit of 2 so it should pass since the ZK circuit will
    // allow this.
    let message_id = message_id + pallas::Base::from(1);
    let a_0 = poseidon_hash([id0.identity_nullifier, id0.identity_trapdoor]);
    let a_1 = poseidon_hash([a_0, external_nullifier, message_id]);
    let x = hash_message("hello again");
    let y = a_0 + x * a_1;

    let internal_nullifier = poseidon_hash([a_1]);

    // Skip the inclusion proof for the demo since we have it above.
    // Make the ZK proof.
    let witnesses = vec![
        Witness::Base(Value::known(id0.identity_nullifier)),
        Witness::Base(Value::known(id0.identity_trapdoor)),
        Witness::Base(Value::known(id0.user_message_limit)),
        Witness::SparseMerklePath(Value::known(path.path)),
        Witness::Base(Value::known(x)),
        Witness::Base(Value::known(message_id)),
        Witness::Base(Value::known(epoch)),
    ];

    let public_inputs = vec![root, external_nullifier, x, y, internal_nullifier];

    print!("[Signal] Creating ZK proof for 0:1... ");
    let now = Instant::now();
    let signal_circuit = ZkCircuit::new(witnesses, &signal_zkbin);
    let proof = Proof::create(&signal_pk, &[signal_circuit], &public_inputs, &mut OsRng).unwrap();
    print!("[{:?}] ", now.elapsed());
    println!("({} bytes)", proof.as_ref().len());

    print!("[Signal] Verifying ZK proof for 0:1... ");
    let now = Instant::now();
    assert!(proof.verify(&signal_vk, &public_inputs).is_ok());
    println!("[{:?}]", now.elapsed());

    // Each user of the protocol must store metadata for each message
    // received by each user, for the given epoch. The data can be
    // deleted when the epoch passes.
    if metadata.is_duplicate(&external_nullifier, &internal_nullifier, &x, &y) {
        println!("[Signal] Duplicate Message!");
        return
    }

    // Add share
    metadata.add_share(external_nullifier, internal_nullifier, x, y);

    // Now we shouldn't be able to create more proofs unless we reuse message_id.
    // This means that some internal_nullifier will have >1 shares, and it should
    // be possible to recover the secret.
    // We reuse the above, just try a different message.
    let x = hash_message("hello again, i'm reusing a message_id");
    let y = a_0 + x * a_1;

    // ZK proof:
    let witnesses = vec![
        Witness::Base(Value::known(id0.identity_nullifier)),
        Witness::Base(Value::known(id0.identity_trapdoor)),
        Witness::Base(Value::known(id0.user_message_limit)),
        Witness::SparseMerklePath(Value::known(path.path)),
        Witness::Base(Value::known(x)),
        Witness::Base(Value::known(message_id)),
        Witness::Base(Value::known(epoch)),
    ];
    let public_inputs = vec![root, external_nullifier, x, y, internal_nullifier];

    print!("[Signal] Creating ZK proof for 0:1 (reused message_id) ... ");
    let now = Instant::now();
    let signal_circuit = ZkCircuit::new(witnesses, &signal_zkbin);
    let proof = Proof::create(&signal_pk, &[signal_circuit], &public_inputs, &mut OsRng).unwrap();
    print!("[{:?}] ", now.elapsed());
    println!("({} bytes)", proof.as_ref().len());

    print!("[Signal] Verifying ZK proof for 0:1 (reused message_id) ... ");
    let now = Instant::now();
    assert!(proof.verify(&signal_vk, &public_inputs).is_ok());
    println!("[{:?}]", now.elapsed());

    // Add share
    metadata.add_share(external_nullifier, internal_nullifier, x, y);

    // Now the internal_nullifier should have been repeated, and the internal
    // nullifier should have 2 (or more) shares.
    // Let's recover them.
    let shares = metadata.get_shares(&external_nullifier, &internal_nullifier);
    println!("{:#?}", shares);
    let secret = sss_recover(&shares);
    println!("secret: {:?}", secret);
    println!("a_0:    {:?}", a_0);
    assert_eq!(secret, a_0);

    // Additionally, it should not be possible to produce (or verify) a ZK
    // proof that exceeds the set message limit for an identity.
    let message_id = message_id + pallas::Base::from(2);
    let a_0 = poseidon_hash([id0.identity_nullifier, id0.identity_trapdoor]);
    let a_1 = poseidon_hash([a_0, external_nullifier, message_id]);
    let x = hash_message("hello again");
    let y = a_0 + x * a_1;

    let internal_nullifier = poseidon_hash([a_1]);

    // Skip the inclusion proof for the demo since we have it above.
    // Make the ZK proof.
    let witnesses = vec![
        Witness::Base(Value::known(id0.identity_nullifier)),
        Witness::Base(Value::known(id0.identity_trapdoor)),
        Witness::Base(Value::known(id0.user_message_limit)),
        Witness::SparseMerklePath(Value::known(path.path)),
        Witness::Base(Value::known(x)),
        Witness::Base(Value::known(message_id)),
        Witness::Base(Value::known(epoch)),
    ];

    let public_inputs = vec![root, external_nullifier, x, y, internal_nullifier];

    println!("[Signal] Creating ZK proof for 0:2 msgid={:?}... ", message_id);
    let signal_circuit = ZkCircuit::new(witnesses, &signal_zkbin);
    let proof = Proof::create(&signal_pk, &[signal_circuit], &public_inputs, &mut OsRng).unwrap();
    assert!(proof.verify(&signal_vk, &public_inputs).is_err());
    println!("[Signal] ZK proof for 0:2 failed as expected");
}
