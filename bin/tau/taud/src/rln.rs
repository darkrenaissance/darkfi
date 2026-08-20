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
        rln::{
            epoch_of, hash_event, Blob, RegistrationAttestation, RlnProver, SignalProvingRequest,
        },
        Event, EventGraphPtr,
    },
    util::memory::log_memory,
    zk::halo2::Field,
    Result,
};
use darkfi_sdk::{crypto::poseidon_hash, pasta::pallas};
use darkfi_serial::{
    async_trait, deserialize_async, serialize_async, SerialDecodable, SerialEncodable,
};
use rand::{CryptoRng, RngCore};
use sled_overlay::sled;
use tracing::{info, warn};

/// Domain-separation tags for credential generation.
pub const RLN_TRAPDOOR_DERIVATION_PATH: pallas::Base = pallas::Base::from_raw([4311, 0, 0, 0]);
pub const RLN_NULLIFIER_DERIVATION_PATH: pallas::Base = pallas::Base::from_raw([4312, 0, 0, 0]);

/// Name of the sled tree that mirrors the currently-active identity.
/// Read on startup to populate the active RLN identity.
pub const ACCOUNTS_DEFAULT_TREE: &str = "tau_account_default";

/// Prefix of the sled tree under which each registered account lives.
pub const ACCOUNTS_DB_PREFIX: &str = "tau_account_";

/// Key inside each account tree holding the serialized [`RlnIdentity`].
pub const ACCOUNTS_KEY_RLN_IDENTITY: &[u8] = b"rln_identity";

/// A user-side RLN identity: long-lived secrets plus a per-epoch
/// send counter.
///
/// The struct is `Copy` so it can be cheaply duplicated after a
/// message slot has been reserved. The canonical mutable copy lives
/// behind the `rln_identity` lock and outbound sends must reserve a
/// slot through [`reserve_rln_message_id_in_store`], which persists
/// `message_id` and `last_epoch` before proof creation. Persisting
/// first means a crash can burn a slot, but cannot roll the counter
/// back and self-slash the identity on restart.
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
            // Default to the pregenerated-identity budget. Fresh
            // identities are useful for generating future genesis bundles,
            // but the live network currently admits only commitments already
            // present in the configured pregenerated set.
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
        let (root, path) = eg.rln_membership_path(&self.commitment()).await?;

        let request = SignalProvingRequest {
            nullifier: self.nullifier,
            trapdoor: self.trapdoor,
            message_id: mid,
            merkle_path: path.path,
            x,
            user_message_limit: self.user_message_limit,
            app_id,
            epoch,
            merkle_root: root,
            external_nullifier: ext_null,
            y,
            internal_nullifier,
        };

        log_memory("before local signal proving");
        info!(
            target: "taud::rln",
            "[RLN] Creating signal proof for event {}",
            event.id(),
        );
        let proof = eg.rln_zk_keys()?.prove_signal(request).await?.proof;
        log_memory("after local signal proving");

        Ok(Blob {
            proof,
            y,
            internal_nullifier,
            user_msg_limit: self.user_message_limit,
            merkle_root: root,
        })
    }
}

/// Result of attempting to reserve the next RLN message slot.
pub enum RlnMessageReservation {
    /// No active RLN identity is configured.
    MissingIdentity,
    /// The active identity has already used its epoch budget.
    BudgetExhausted,
    /// A message slot was persisted and can be used to build a proof.
    Reserved { identity: RlnIdentity, message_id: u64 },
}

/// Persist the active RLN counter to the default mirror and matching account tree.
pub async fn persist_rln_identity_counter(
    sled_db: &sled::Db,
    identity: &RlnIdentity,
) -> Result<()> {
    let encoded = serialize_async(identity).await;
    let active_commitment = identity.commitment();
    let mut updated_account = false;

    for raw in sled_db.tree_names() {
        let bytes: &[u8] = raw.as_ref();
        let Ok(name) = std::str::from_utf8(bytes) else { continue };
        let Some(account_name) = name.strip_prefix(ACCOUNTS_DB_PREFIX) else { continue };
        if account_name == "default" || account_name.is_empty() {
            continue
        }

        let tree = sled_db.open_tree(name)?;
        let Some(blob) = tree.get(ACCOUNTS_KEY_RLN_IDENTITY)? else { continue };
        let Ok(stored): std::result::Result<RlnIdentity, _> = deserialize_async(&blob).await else {
            continue
        };
        if stored.commitment() == active_commitment {
            tree.insert(ACCOUNTS_KEY_RLN_IDENTITY, encoded.clone())?;
            updated_account = true;
        }
    }

    if !updated_account {
        warn!(
            target: "taud::rln",
            "active RLN identity has no matching account tree; persisting default mirror only",
        );
    }

    let default_db = sled_db.open_tree(ACCOUNTS_DEFAULT_TREE)?;
    default_db.insert(ACCOUNTS_KEY_RLN_IDENTITY, encoded)?;
    sled_db.flush_async().await?;
    Ok(())
}

/// Reserve the next RLN message ID and persist it before proof creation.
pub async fn reserve_rln_message_id_in_store(
    sled_db: &sled::Db,
    active: &mut Option<RlnIdentity>,
    now_millis: u64,
) -> Result<RlnMessageReservation> {
    let Some(current) = active else { return Ok(RlnMessageReservation::MissingIdentity) };

    let mut updated = *current;
    let Some(message_id) = updated.next_message_id(now_millis) else {
        return Ok(RlnMessageReservation::BudgetExhausted)
    };

    persist_rln_identity_counter(sled_db, &updated).await?;
    *current = updated;

    Ok(RlnMessageReservation::Reserved { identity: updated, message_id })
}

/// Load the active (default-mirror) RLN identity, if any.
pub async fn load_default_rln_identity(sled_db: &sled::Db) -> Result<Option<RlnIdentity>> {
    let default_db = sled_db.open_tree(ACCOUNTS_DEFAULT_TREE)?;
    let Some(blob) = default_db.get(ACCOUNTS_KEY_RLN_IDENTITY)? else {
        if default_db.is_empty() {
            return Ok(None)
        }

        return Err(darkfi::Error::ParseFailed("Default RLN account is missing identity record"))
    };

    let identity: RlnIdentity = deserialize_async(&blob)
        .await
        .map_err(|_| darkfi::Error::ParseFailed("Default RLN account identity is corrupted"))?;

    Ok(Some(identity))
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkfi_sdk::pasta::pallas;
    use darkfi_serial::deserialize_async;

    #[test]
    fn load_default_rln_identity_returns_none_for_empty_tree() {
        smol::block_on(async {
            let sled_db = sled::Config::new().temporary(true).open().unwrap();

            let identity = load_default_rln_identity(&sled_db).await.unwrap();

            assert!(identity.is_none());
        })
    }

    #[test]
    fn rln_message_reservation_persists_default_and_account_counters() {
        smol::block_on(async {
            let sled_db = sled::Config::new().temporary(true).open().unwrap();
            let account = sled_db.open_tree(format!("{ACCOUNTS_DB_PREFIX}alice")).unwrap();
            let default = sled_db.open_tree(ACCOUNTS_DEFAULT_TREE).unwrap();
            let identity = RlnIdentity {
                nullifier: pallas::Base::from(0xabc_u64),
                trapdoor: pallas::Base::from(0xdef_u64),
                user_message_limit: 2,
                message_id: 0,
                last_epoch: 0,
            };
            let encoded = serialize_async(&identity).await;
            account.insert(ACCOUNTS_KEY_RLN_IDENTITY, encoded.clone()).unwrap();
            default.insert(ACCOUNTS_KEY_RLN_IDENTITY, encoded).unwrap();

            let now = 1_704_067_800_000;
            let mut active = Some(identity);
            let reservation =
                reserve_rln_message_id_in_store(&sled_db, &mut active, now).await.unwrap();
            let RlnMessageReservation::Reserved { identity: reserved, message_id } = reservation
            else {
                panic!("expected reservation")
            };
            assert_eq!(message_id, 0);
            assert_eq!(reserved.message_id, 1);
            assert_eq!(reserved.last_epoch, epoch_of(now));

            let stored_default: RlnIdentity =
                deserialize_async(&default.get(ACCOUNTS_KEY_RLN_IDENTITY).unwrap().unwrap())
                    .await
                    .unwrap();
            let stored_account: RlnIdentity =
                deserialize_async(&account.get(ACCOUNTS_KEY_RLN_IDENTITY).unwrap().unwrap())
                    .await
                    .unwrap();
            assert_eq!(stored_default.message_id, 1);
            assert_eq!(stored_account.message_id, 1);

            let reservation =
                reserve_rln_message_id_in_store(&sled_db, &mut active, now).await.unwrap();
            let RlnMessageReservation::Reserved { message_id, .. } = reservation else {
                panic!("expected second reservation")
            };
            assert_eq!(message_id, 1);

            let exhausted =
                reserve_rln_message_id_in_store(&sled_db, &mut active, now).await.unwrap();
            assert!(matches!(exhausted, RlnMessageReservation::BudgetExhausted));
        })
    }
}
