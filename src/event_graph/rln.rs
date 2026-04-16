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

//! Rate-Limit Nullifier (RLN) v2 integration for the Event Graph.
//!
//! RLN lets anonymous users post to the DAG at a configurable rate.
//! If a user exceeds their rate limit (by reusing a message slot
//! within the same epoch), their shares reveal their secret key via
//! Shamir's Secret Sharing, and anyone can produce a slashing proof
//! to remove them from the identity tree.

use std::{collections::BTreeMap, io::Cursor};

use darkfi_sdk::{
    crypto::{
        pasta_prelude::{FromUniformBytes, PrimeField},
        poseidon_hash,
        smt::{MemoryStorageFp, PoseidonFp, SmtMemoryFp, EMPTY_NODES_FP},
    },
    pasta::pallas,
};
use darkfi_serial::{async_trait, FutAsyncWriteExt, SerialDecodable, SerialEncodable};
use halo2_proofs::{arithmetic::Field, circuit::Value};
use rand::rngs::OsRng;
use sled_overlay::sled;
use tracing::info;

use crate::{
    event_graph::Event,
    zk::{empty_witnesses, Proof, ProvingKey, VerifyingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Error, Result,
};

pub const RLN2_REGISTER_ZKBIN: &[u8] = include_bytes!("proof/rlnv2-diff-register.zk.bin");
pub const RLN2_SIGNAL_ZKBIN: &[u8] = include_bytes!("proof/rlnv2-diff-signal.zk.bin");
pub const RLN2_SLASH_ZKBIN: &[u8] = include_bytes!("proof/rlnv2-diff-slash.zk.bin");

/// RLN epoch genesis in millis.
/// Used as the time-zero reference for epoch numbering.
pub const RLN_GENESIS: u64 = 1_738_688_400_000;

/// Duration of one RLN epoch in millis (10 minutes).
pub const RLN_EPOCH_LEN: u64 = 600_000;

/// Ephemeral data attached to an [`EventPut`] when RLN is active.
#[derive(SerialEncodable, SerialDecodable)]
pub struct Blob {
    /// The RLN signal proof.
    pub proof: Proof,
    /// The `y` share value: `y = a_0 + x * a_1`.
    pub y: pallas::Base,
    /// Nullifier derived from `(identity, epoch, message_id)`.
    pub internal_nullifier: pallas::Base,
    /// The user's per-registration message limit.
    pub user_msg_limit: u64,
}

/// An entry in the static DAG representing an identity event.
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub enum RLNNode {
    /// A new identity commitment being registered.
    Registration(pallas::Base),
    /// An identity commitment being slashed (removed).
    Slashing(pallas::Base),
}

/// ZK key cache.
pub struct ZkKeys {
    /// Verifying key for identity registration proofs.
    pub register_vk: VerifyingKey,
    /// Verifying key for signal (rate-limit) proofs.
    pub signal_vk: VerifyingKey,
    /// Verifying key for slash proofs.
    pub slash_vk: VerifyingKey,
    /// Reference to the sled DB so we can lazy-load the proving keys.
    sled_db: sled::Db,
}

impl ZkKeys {
    /// Ensure all keys exist in sled and load only the verifying
    /// keys into memory.
    pub fn build_and_load(sled_db: &sled::Db) -> Result<Self> {
        ensure_key(sled_db, "rlnv2-diff-register-vk", RLN2_REGISTER_ZKBIN, KeyKind::Vk)?;
        ensure_key(sled_db, "rlnv2-diff-signal-vk", RLN2_SIGNAL_ZKBIN, KeyKind::Vk)?;
        ensure_key(sled_db, "rlnv2-diff-slash-pk", RLN2_SLASH_ZKBIN, KeyKind::Pk)?;
        ensure_key(sled_db, "rlnv2-diff-slash-vk", RLN2_SLASH_ZKBIN, KeyKind::Vk)?;

        Ok(Self {
            register_vk: read_vk(sled_db, "rlnv2-diff-register-vk", RLN2_REGISTER_ZKBIN)?,
            signal_vk: read_vk(sled_db, "rlnv2-diff-signal-vk", RLN2_SIGNAL_ZKBIN)?,
            slash_vk: read_vk(sled_db, "rlnv2-diff-slash-vk", RLN2_SLASH_ZKBIN)?,
            sled_db: sled_db.clone(),
        })
    }

    /// Load the slash proving key from sled.
    /// This is expensive memory-wise and should only be called when
    /// a slash proof is about to be created.
    pub fn load_slash_pk(&self) -> Result<ProvingKey> {
        read_pk(&self.sled_db, "rlnv2-diff-slash-pk", RLN2_SLASH_ZKBIN)
    }
}

/// Mutable RLN state shared across all protocol instances via
/// `EventGraph::rln_state`. Each peer connection's protocol handler
/// accesses this through a write lock so that duplicate/reuse
/// detection works regardless of which peer relayed the event.
pub struct RlnState {
    /// Per-nullifier share tracking for the current epoch.
    pub metadata: MessageMetadata,
    /// The epoch for which `metadata` is valid. When the epoch
    /// changes, the metadata is reset.
    pub current_epoch: u64,
}

impl RlnState {
    pub fn new() -> Self {
        Self { metadata: MessageMetadata::new(), current_epoch: 0 }
    }
}

impl Default for RlnState {
    fn default() -> Self {
        Self::new()
    }
}

/// The set of currently registered RLN identities, stored as a Sparse
/// Merkle Tree (SMT).
///
/// Persistence model: leaf commitments are stored in a dedicated sled
/// tree (`rln-identity-leaves`). The in-memory SMT is rebuilt from
/// these leaves on startup.
pub struct IdentityState {
    /// In-memory SMT for fast root computation and membership proofs.
    smt: SmtMemoryFp,
    /// Sled tree holding the persisted leaf set.
    leaves: sled::Tree,
}

impl IdentityState {
    /// Create a new identity state, restoring leaves from sled if present.
    pub fn new(sled_db: &sled::Db) -> Result<Self> {
        let hasher = PoseidonFp::new();
        let store = MemoryStorageFp::new();
        let mut smt = SmtMemoryFp::new(store, hasher, &EMPTY_NODES_FP);

        let leaves = sled_db.open_tree("rln-identity-leaves")?;

        // Rebuild SMT from persisted leaves
        let mut batch = vec![];
        for item in leaves.iter() {
            let (_, val) = item?;
            let mut repr = [0u8; 32];
            repr.copy_from_slice(&val);
            if let Some(c) = pallas::Base::from_repr(repr).into() {
                batch.push((c, c));
            }
        }

        if !batch.is_empty() {
            info!(
                target: "event_graph::rln",
                "[RLN] Restoring {} identities from sled", batch.len(),
            );
            smt.insert_batch(batch)?;
        }

        Ok(Self { smt, leaves })
    }

    /// Register a new identity. Writes to both the in-memory SMT
    /// and the sled persistence tree.
    pub fn register(&mut self, commitment: pallas::Base) -> Result<()> {
        self.leaves.insert(commitment.to_repr(), commitment.to_repr().as_ref())?;
        self.smt.insert_batch(vec![(commitment, commitment)])?;
        Ok(())
    }

    /// Slash (remove) an identity.
    pub fn slash(&mut self, commitment: pallas::Base) -> Result<()> {
        self.leaves.remove(commitment.to_repr())?;
        self.smt.remove_leaves(vec![(commitment, commitment)])?;
        Ok(())
    }

    /// Current Merkle root of the identity tree.
    pub fn root(&self) -> pallas::Base {
        self.smt.root()
    }

    /// Generate a membership proof for `commitment`.
    pub fn prove_membership(&self, commitment: &pallas::Base) -> darkfi_sdk::crypto::smt::PathFp {
        self.smt.prove_membership(commitment)
    }
}

/// Hash an event's header ID into a field element suitable for use
/// as the `x` coordinate in the RLN polynomial evaluation.
pub fn hash_event(event: &Event) -> pallas::Base {
    let mut buf = [0u8; 64];
    buf[..blake3::OUT_LEN].copy_from_slice(event.header.id().as_bytes());
    pallas::Base::from_uniform_bytes(&buf)
}

/// Map a UNIX-millis timestamp to the nearest RLN epoch boundary.
///
/// Returns `0` if the timestamp predates [`RLN_GENESIS`], avoiding
/// underflow panics on malicious timestamps.
pub fn closest_epoch(timestamp: u64) -> u64 {
    let Some(diff) = timestamp.checked_sub(RLN_GENESIS) else { return 0 };
    let idx = (diff as f64 / RLN_EPOCH_LEN as f64).round() as u64;
    RLN_GENESIS.saturating_add(idx.saturating_mul(RLN_EPOCH_LEN))
}

#[derive(Debug, Clone)]
struct ShareData {
    /// Collected `(x, y)` share pairs for a single internal nullifier.
    shares: Vec<(pallas::Base, pallas::Base)>,
}

/// Per-epoch tracking of RLN shares, keyed by nullifier pairs.
///
/// Each `(external_nullifier, internal_nullifier)` maps to the set
/// of `(x, y)` shares seen so far.
/// This allows detecting:
/// * **Duplicates** - the exact same `(x, y)` pair arriving twice
///   (the event is just dropped).
/// * **Slot reuse** - a different `(x, y)` for the same internal
///   nullifier (the user reused a message slot, triggering slashing).
#[derive(Debug, Default)]
pub struct MessageMetadata {
    data: BTreeMap<pallas::Base, BTreeMap<pallas::Base, ShareData>>,
}

impl MessageMetadata {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a new share.
    pub fn add_share(
        &mut self,
        ext_null: pallas::Base,
        int_null: pallas::Base,
        x: pallas::Base,
        y: pallas::Base,
    ) -> Result<()> {
        self.data
            .entry(ext_null)
            .or_default()
            .entry(int_null)
            .or_insert_with(|| ShareData { shares: vec![] })
            .shares
            .push((x, y));
        Ok(())
    }

    /// Retrieve all shares for a given nullifier pair.
    pub fn get_shares(
        &self,
        ext_null: &pallas::Base,
        int_null: &pallas::Base,
    ) -> Vec<(pallas::Base, pallas::Base)> {
        self.data
            .get(ext_null)
            .and_then(|m| m.get(int_null))
            .map(|sd| sd.shares.clone())
            .unwrap_or_default()
    }

    /// Check whether the exact `(x, y)` pair is already recorded.
    ///
    /// This compares pairs - not independent coordinates - to avoid
    /// false positives from cross-matching different shares.
    pub fn is_duplicate(
        &self,
        ext_null: &pallas::Base,
        int_null: &pallas::Base,
        x: &pallas::Base,
        y: &pallas::Base,
    ) -> bool {
        self.data
            .get(ext_null)
            .and_then(|m| m.get(int_null))
            .map(|sd| sd.shares.iter().any(|(sx, sy)| sx == x && sy == y))
            .unwrap_or(false)
    }

    /// Check whether any share has been recorded for this nullifier
    /// pair in the current epoch.
    ///
    /// In RLNv2, each `message_id` produces a unique `internal_nullifier`.
    /// A repeated `internal_nullifier` means the user reused the same
    /// message slot, which is a protocol violation that enables secret
    /// recovery via SSS.
    pub fn is_reused(&self, ext_null: &pallas::Base, int_null: &pallas::Base) -> bool {
        self.data.get(ext_null).map(|m| m.contains_key(int_null)).unwrap_or(false)
    }
}

/// Create a ZK proof that a user's secret has been recovered (via SSS)
/// and they should be slashed from the identity tree.
pub fn create_slash_proof(
    secret: pallas::Base,
    user_msg_limit: u64,
    identity_state: &mut IdentityState,
    slash_pk: &ProvingKey,
) -> Result<(Proof, pallas::Base)> {
    let ish = poseidon_hash([secret, user_msg_limit.into()]);
    let commitment = poseidon_hash([ish]);
    let root = identity_state.root();
    let path = identity_state.prove_membership(&commitment);

    let witnesses = vec![
        Witness::Base(Value::known(secret)),
        Witness::Base(Value::known(pallas::Base::from(user_msg_limit))),
        Witness::SparseMerklePath(Value::known(path.path)),
    ];
    let pi = vec![secret, pallas::Base::from(user_msg_limit), root];
    let zkbin = ZkBinary::decode(RLN2_SLASH_ZKBIN, false)?;
    let circuit = ZkCircuit::new(witnesses, &zkbin);
    let proof = Proof::create(slash_pk, &[circuit], &pi, &mut OsRng)
        .map_err(|e| Error::Custom(format!("Slash proof creation failed: {e}")))?;
    Ok((proof, root))
}

/// Recover the secret from two or more `(x, y)` Shamir shares using
/// Lagrange interpolation.
///
/// Returns an error if fewer than 2 shares are provided or if any two
/// shares have the same x-coordinate (which would cause a zero
/// division).
pub fn sss_recover(shares: &[(pallas::Base, pallas::Base)]) -> Result<pallas::Base> {
    if shares.len() < 2 {
        return Err(Error::Custom("Need >1 share for SSS recovery".into()))
    }

    // Guard against duplicate x-coordinates
    for i in 0..shares.len() {
        for j in (i + 1)..shares.len() {
            if shares[i].0 == shares[j].0 {
                return Err(Error::Custom("Duplicate x-coordinates in SSS shares".into()))
            }
        }
    }

    let mut secret = pallas::Base::zero();
    for (j, sj) in shares.iter().enumerate() {
        let mut basis = pallas::Base::one();
        for (i, si) in shares.iter().enumerate() {
            if i != j {
                basis *= si.0 * (si.0 - sj.0).invert().unwrap();
            }
        }
        secret += basis * sj.1;
    }

    Ok(secret)
}

enum KeyKind {
    Pk,
    Vk,
}

/// Build a key into sled if it doesn't already exist.
fn ensure_key(sled_db: &sled::Db, key: &str, zkbin_bytes: &[u8], kind: KeyKind) -> Result<()> {
    if sled_db.get(key)?.is_some() {
        return Ok(())
    }

    let zkbin = ZkBinary::decode(zkbin_bytes, false)?;
    let circuit = ZkCircuit::new(empty_witnesses(&zkbin)?, &zkbin);
    info!(target: "event_graph::rln", "[RLN] Building {key}");

    let mut buf = vec![];
    match kind {
        KeyKind::Pk => {
            let pk = ProvingKey::build(zkbin.k, &circuit);
            pk.write(&mut buf)?;
        }
        KeyKind::Vk => {
            let vk = VerifyingKey::build(zkbin.k, &circuit);
            vk.write(&mut buf)?;
        }
    }
    sled_db.insert(key, buf)?;
    Ok(())
}

fn read_vk(sled_db: &sled::Db, key: &str, zkbin_bytes: &[u8]) -> Result<VerifyingKey> {
    let bytes = sled_db.get(key)?.ok_or_else(|| Error::Custom(format!("{key} not found")))?;
    let zkbin = ZkBinary::decode(zkbin_bytes, false)?;
    let circuit = ZkCircuit::new(empty_witnesses(&zkbin)?, &zkbin);
    Ok(VerifyingKey::read(&mut Cursor::new(bytes), circuit)?)
}

fn read_pk(sled_db: &sled::Db, key: &str, zkbin_bytes: &[u8]) -> Result<ProvingKey> {
    let bytes = sled_db.get(key)?.ok_or_else(|| Error::Custom(format!("{key} not found")))?;
    let zkbin = ZkBinary::decode(zkbin_bytes, false)?;
    let circuit = ZkCircuit::new(empty_witnesses(&zkbin)?, &zkbin);
    Ok(ProvingKey::read(&mut Cursor::new(bytes), circuit)?)
}
