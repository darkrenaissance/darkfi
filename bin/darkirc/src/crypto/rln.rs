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

use darkfi::{
    event_graph::{
        rln::{epoch_of, hash_event, Blob, RegistrationAttestation, RLN2_SIGNAL_ZKBIN},
        Event, EventGraphPtr,
    },
    zk::{
        halo2::{Field, Value},
        Proof, Witness, ZkCircuit,
    },
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{crypto::poseidon_hash, pasta::pallas};
use darkfi_serial::{async_trait, SerialDecodable, SerialEncodable};
use rand::{rngs::OsRng, CryptoRng, RngCore};
use tracing::info;

/// Domain-separation tags for credential generation.
pub const RLN_TRAPDOOR_DERIVATION_PATH: pallas::Base = pallas::Base::from_raw([4211, 0, 0, 0]);
pub const RLN_NULLIFIER_DERIVATION_PATH: pallas::Base = pallas::Base::from_raw([4212, 0, 0, 0]);

/// A user-side RLN identity: long-lived secrets plus an in-memory
/// per-epoch send counter.
///
/// The struct is `Copy` so it can be cheaply duplicated when handed
/// off to client tasks; the per-epoch counter (`message_id`,
/// `last_epoch`) is therefore expected to be tracked by whichever
/// task owns the canonical mutable copy. In the typical DarkIRC
/// configuration that's the `IrcServer::rln_identity` field
/// (`RwLock<Option<RlnIdentity>>`).
///
/// `message_id` and `last_epoch` are not persisted to disk on
/// shutdown. An RLN epoch is `RLN_EPOCH_LEN` (10 minutes); the
/// worst case of a node restart is that the counter resets to 0
/// for the current epoch, which only risks a slash if the user
/// actually sent distinct messages with the same message_id in the
/// same epoch - which generally requires hot-restarting at sub-
/// second cadence. If/when that becomes an operational concern,
/// persist the counter through a `next_message_id_persisted`-style
/// helper at the call site.
#[derive(Copy, Clone, SerialEncodable, SerialDecodable)]
pub struct RlnIdentity {
    pub nullifier: pallas::Base,
    pub trapdoor: pallas::Base,
    pub user_message_limit: u64,
    /// Monotonic counter within the current epoch. Reset whenever
    /// `last_epoch` advances.
    pub message_id: u64,
    /// Last epoch we observed. Bookkeeping for the counter reset
    /// above; not used cryptographically.
    pub last_epoch: u64,
}

impl RlnIdentity {
    /// Generate a fresh identity.
    pub fn new(mut rng: impl CryptoRng + RngCore) -> Self {
        Self {
            nullifier: poseidon_hash([
                RLN_NULLIFIER_DERIVATION_PATH,
                pallas::Base::random(&mut rng),
            ]),
            trapdoor: poseidon_hash([RLN_TRAPDOOR_DERIVATION_PATH, pallas::Base::random(&mut rng)]),
            // Default to the free-tier cap. The operator can request
            // a higher limit at registration time (subject to the
            // attestation gating in `RegistrationAttestation::permits`,
            // which currently only honours the free-tier cap until
            // staking lands).
            user_message_limit: RegistrationAttestation::SPECIAL_TIER_LIMIT,
            message_id: 0,
            last_epoch: 0,
        }
    }

    /// `identity_secret = poseidon(nullifier, trapdoor)`. Internal
    /// to the RLN-V2 algebra.
    pub fn identity_secret(&self) -> pallas::Base {
        poseidon_hash([self.nullifier, self.trapdoor])
    }

    /// `identity_secret_hash = poseidon(identity_secret, user_message_limit)`.
    /// This is the value recovered by SSS during a slash, NOT the
    /// raw secret tuple.
    pub fn identity_secret_hash(&self) -> pallas::Base {
        poseidon_hash([self.identity_secret(), pallas::Base::from(self.user_message_limit)])
    }

    /// `commitment = poseidon(identity_secret_hash)`. The leaf in
    /// the SMT.
    pub fn commitment(&self) -> pallas::Base {
        poseidon_hash([self.identity_secret_hash()])
    }

    /// Advance the per-epoch counter for a signal at the given
    /// timestamp. Returns `None` if the user has already burnt
    /// their `user_message_limit` for this epoch (in which case the
    /// caller should drop the message rather than emit a signal
    /// that would slash the identity).
    ///
    /// On epoch rollover the counter resets and a fresh slot 0 is
    /// returned.
    pub fn next_message_id(&mut self, now_millis: u64) -> Option<u64> {
        let epoch = epoch_of(now_millis);
        if epoch != self.last_epoch {
            self.last_epoch = epoch;
            self.message_id = 0;
        }
        if self.message_id >= self.user_message_limit {
            return None
        }
        let m = self.message_id;
        self.message_id += 1;
        Some(m)
    }

    /// Build a [`RegistrationBlob`] suitable for broadcast as a
    /// `StaticPut`. The proving key comes from the EventGraph's
    /// shared `ZkKeys` cache.
    // pub fn create_registration(&self, eg: &EventGraphPtr) -> Result<RegistrationBlob> {
    //     let zkbin = ZkBinary::decode(RLN2_REGISTER_ZKBIN, false)?;

    //     // Witness order MUST match the rlnv2-diff-register.zk circuit.
    //     let witnesses = vec![
    //         Witness::Base(Value::known(self.nullifier)),
    //         Witness::Base(Value::known(self.trapdoor)),
    //         Witness::Base(Value::known(pallas::Base::from(self.user_message_limit))),
    //         Witness::Base(Value::known(pallas::Base::from(MAX_MSG_LIMIT))),
    //     ];
    //     // Public-input order MUST match `constrain_instance` in the
    //     // same .zk file.
    //     let pi = vec![
    //         self.commitment(),
    //         pallas::Base::from(self.user_message_limit),
    //         pallas::Base::from(MAX_MSG_LIMIT),
    //     ];
    //     let circuit = ZkCircuit::new(witnesses, &zkbin);
    //     let pk = eg.zk_keys.load_register_pk()?;

    //     info!(
    //         target: "darkirc::crypto::rln",
    //         "[RLN] Creating registration proof for commitment {:?}",
    //         self.commitment(),
    //     );
    //     let proof = Proof::create(&pk, &[circuit], &pi, &mut OsRng)?;

    //     Ok(RegistrationBlob {
    //         proof,
    //         user_message_limit: self.user_message_limit,
    //         max_message_limit: MAX_MSG_LIMIT,
    //         attestation: RegistrationAttestation::SPECIAL,
    //     })
    // }

    /// Build a signal [`Blob`] for the given event using `message_id`.
    ///
    /// The merkle root and inclusion path come from the
    /// EventGraph's canonical [`IdentityState`] via
    /// [`EventGraph::rln_membership_path`] - the verifier and the
    /// prover therefore agree on the root by construction, with no
    /// risk of the client and the EG drifting out of sync.
    ///
    /// [`IdentityState`]: darkfi::event_graph::rln::IdentityState
    /// [`EventGraph::rln_membership_path`]: darkfi::event_graph::EventGraph::rln_membership_path
    pub async fn create_signal(
        &self,
        event: &Event,
        message_id: u64,
        eg: &EventGraphPtr,
    ) -> Result<Blob> {
        // RLN external nullifier: ties the message to (epoch, app).
        // Cross-app isolation comes from `app_id` differing per
        // EventGraph deployment (derived from
        // config.genesis_contents).
        let app_id = eg.rln_app_id().as_field();
        let epoch = pallas::Base::from(epoch_of(event.header.timestamp));
        let mid = pallas::Base::from(message_id);
        let ext_null = poseidon_hash([epoch, app_id]);

        // Rate-limit polynomial: y = a_0 + x * a_1.
        // a_0 is identity_secret_hash; a_1 is bound to (a_0,
        // ext_null, message_id). Two distinct (x, y) for the same
        // internal nullifier let SSS recover a_0, which is what
        // enables slashing.
        let a_0 = self.identity_secret_hash();
        let a_1 = poseidon_hash([a_0, ext_null, mid]);
        let internal_nullifier = poseidon_hash([a_1]);
        let x = hash_event(event);
        let y = a_0 + x * a_1;

        // Canonical membership path via the EG.
        let (root, path) = eg.rln_membership_path(&self.commitment()).await;

        let zkbin = ZkBinary::decode(RLN2_SIGNAL_ZKBIN, false)?;

        // Witness order MUST match `witness` declarations in
        let witnesses = vec![
            Witness::Base(Value::known(self.nullifier)),
            Witness::Base(Value::known(self.trapdoor)),
            Witness::Base(Value::known(mid)),
            Witness::SparseMerklePath(Value::known(path.path)),
            Witness::Base(Value::known(x)),
            Witness::Base(Value::known(pallas::Base::from(self.user_message_limit))),
            Witness::Base(Value::known(app_id)),
            Witness::Base(Value::known(epoch)),
        ];
        // PI order MUST match `constrain_instance` in the .zk file.
        let pi = vec![
            root,
            ext_null,
            pallas::Base::from(self.user_message_limit),
            x,
            y,
            internal_nullifier,
        ];
        let circuit = ZkCircuit::new(witnesses, &zkbin);
        let pk = eg.zk_keys.load_signal_pk()?;

        info!(
            target: "darkirc::crypto::rln",
            "[RLN] Creating signal proof for event {}",
            event.id(),
        );
        let proof = Proof::create(&pk, &[circuit], &pi, &mut OsRng)?;

        Ok(Blob {
            proof,
            y,
            internal_nullifier,
            user_msg_limit: self.user_message_limit,
            merkle_root: root,
        })
    }
}
