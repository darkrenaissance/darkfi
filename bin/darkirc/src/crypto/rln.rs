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

use std::time::UNIX_EPOCH;

use darkfi::{
    event_graph::Event,
    zk::{
        halo2::{Field, Value},
        Proof, ProvingKey, Witness, ZkCircuit,
    },
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{pasta_prelude::FromUniformBytes, poseidon_hash, smt::SmtMemoryFp},
    pasta::pallas,
};
use darkfi_serial::{async_trait, SerialDecodable, SerialEncodable};
use rand::{rngs::OsRng, CryptoRng, RngCore};
use tracing::info;

pub const RLN_TRAPDOOR_DERIVATION_PATH: pallas::Base = pallas::Base::from_raw([4211, 0, 0, 0]);
pub const RLN_NULLIFIER_DERIVATION_PATH: pallas::Base = pallas::Base::from_raw([4212, 0, 0, 0]);

/// RLN epoch genesis in millis
pub const RLN_GENESIS: u64 = 1_738_688_400_000;
/// RLN epoch length in millis
pub const RLN_EPOCH_LEN: u64 = 600_000; // 10 min

pub const RLN2_REGISTER_ZKBIN: &[u8] =
    include_bytes!("../../../../src/event_graph/proof/rlnv2-diff-register.zk.bin");
pub const RLN2_SIGNAL_ZKBIN: &[u8] =
    include_bytes!("../../../../src/event_graph/proof/rlnv2-diff-signal.zk.bin");

/// TODO: this is arbitrary it should be based on stake
pub const MAX_MSG_LIMIT: u64 = 100;

/// Find closest epoch to given timestamp
pub fn closest_epoch(timestamp: u64) -> u64 {
    let time_diff = timestamp - RLN_GENESIS;
    let epoch_idx = time_diff as f64 / RLN_EPOCH_LEN as f64;
    let rounded = epoch_idx.round() as i64;
    RLN_GENESIS + (rounded * RLN_EPOCH_LEN as i64) as u64
}

/// Hash message/event modulo `Fp`
pub fn hash_event(event: &Event) -> pallas::Base {
    let mut buf = [0u8; 64];
    buf[..blake3::OUT_LEN].copy_from_slice(event.header.id().as_bytes());
    pallas::Base::from_uniform_bytes(&buf)
}

#[derive(Copy, Clone, SerialEncodable, SerialDecodable)]
pub struct RlnIdentity {
    pub nullifier: pallas::Base,
    pub trapdoor: pallas::Base,
    pub user_message_limit: u64,
    /// This should increment during a single epoch and reset on new epochs
    pub message_id: u64,
    /// Last known epoch
    pub last_epoch: u64,
}

impl RlnIdentity {
    pub fn new(mut rng: impl CryptoRng + RngCore) -> Self {
        Self {
            nullifier: poseidon_hash([
                RLN_NULLIFIER_DERIVATION_PATH,
                pallas::Base::random(&mut rng),
            ]),
            trapdoor: poseidon_hash([RLN_TRAPDOOR_DERIVATION_PATH, pallas::Base::random(&mut rng)]),
            user_message_limit: MAX_MSG_LIMIT,
            message_id: 0,
            last_epoch: closest_epoch(UNIX_EPOCH.elapsed().unwrap().as_millis() as u64),
        }
    }

    pub fn commitment(&self) -> pallas::Base {
        let identity_secret = poseidon_hash([self.nullifier, self.trapdoor]);
        let identity_secret_hash = poseidon_hash([identity_secret, self.user_message_limit.into()]);

        poseidon_hash([identity_secret_hash])
    }

    pub fn create_register_proof(
        &self,
        event: &Event,
        identities_tree: &mut SmtMemoryFp,
        register_pk: &ProvingKey,
    ) -> Result<Proof> {
        let witnesses = vec![
            Witness::Base(Value::known(self.nullifier)),
            Witness::Base(Value::known(self.trapdoor)),
            Witness::Base(Value::known(pallas::Base::from(self.user_message_limit))),
            Witness::Base(Value::known(pallas::Base::from(MAX_MSG_LIMIT))),
        ];

        let commitment = self.commitment();
        let public_inputs = vec![commitment, pallas::Base::from(self.user_message_limit)];

        info!(target: "crypto::rln::create_register_proof", "[RLN] Creating register proof for account {}", event.header.id());
        let register_zkbin = ZkBinary::decode(RLN2_REGISTER_ZKBIN, false)?;
        let register_circuit = ZkCircuit::new(witnesses, &register_zkbin);

        let proof =
            Proof::create(&register_pk, &[register_circuit], &public_inputs, &mut OsRng).unwrap();

        let leaf = vec![commitment];
        let leaf: Vec<_> = leaf.into_iter().map(|l| (l, l)).collect();
        // TODO: Recipients should verify that identity doesn't exist already before insert.
        identities_tree.insert_batch(leaf.clone()).unwrap(); // leaf == pos
        Ok(proof)
    }

    pub fn create_signal_proof(
        &self,
        event: &Event,
        identity_tree: &SmtMemoryFp,
        signal_pk: &ProvingKey,
    ) -> Result<(Proof, pallas::Base, pallas::Base, u64)> {
        // 1. Construct share
        let rln_app_identifier = pallas::Base::from(1000);
        let epoch = pallas::Base::from(closest_epoch(event.header.timestamp));
        let message_id = pallas::Base::from(self.message_id);
        let external_nullifier = poseidon_hash([epoch, rln_app_identifier]);
        let a_0 = poseidon_hash([self.nullifier, self.trapdoor]);
        let a_1 = poseidon_hash([a_0, external_nullifier, message_id]);
        let x = hash_event(event);
        let y = a_0 + x * a_1;

        let internal_nullifier = poseidon_hash([a_1]);

        // 2. Inclusion proof
        let commitment = self.commitment();
        let identity_root = identity_tree.root();
        let identity_path = identity_tree.prove_membership(&commitment);
        // TODO: Delete me later
        assert!(identity_path.verify(&identity_root, &commitment, &commitment));

        // 3. Create ZK proof
        let witnesses = vec![
            Witness::Base(Value::known(self.nullifier)),
            Witness::Base(Value::known(self.trapdoor)),
            Witness::Base(Value::known(pallas::Base::from(self.user_message_limit))),
            Witness::SparseMerklePath(Value::known(identity_path.path)),
            Witness::Base(Value::known(x)),
            Witness::Base(Value::known(message_id)),
            Witness::Base(Value::known(epoch)),
        ];

        let public_inputs = vec![identity_root, external_nullifier, x, y, internal_nullifier];

        info!(target: "crypto::rln::create_signal_proof", "[RLN] Creating signal proof for event {}", event.header.id());
        let signal_zkbin = ZkBinary::decode(RLN2_SIGNAL_ZKBIN, false)?;
        let signal_circuit = ZkCircuit::new(witnesses.clone(), &signal_zkbin);

        let proof = Proof::create(signal_pk, &[signal_circuit], &public_inputs, &mut OsRng)?;
        Ok((proof, y, internal_nullifier, self.user_message_limit))
        // Ok((proof, public_inputs))
    }
}
