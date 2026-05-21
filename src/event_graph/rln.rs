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

//! Rate-Limit Nullifier (RLN-V2, Diff variant) integration for the
//! Event Graph.
//!
//! RLN lets anonymous users post to the DAG at a configurable rate.
//! If a user exceeds their rate limit (by reusing a `message_id`
//! within the same epoch), their shares reveal their `identity_secret_hash`
//! via Shamir's Secret Sharing, and anyone can produce a slashing
//! proof to remove them from the identity tree.

use std::{
    collections::{BTreeMap, VecDeque},
    io::Cursor,
};

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

use super::Event;
use crate::{
    event_graph::genesis_commits::GENESIS_COMMITMENTS_REPR,
    zk::{empty_witnesses, Proof, ProvingKey, VerifyingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Error, Result,
};

pub const RLN2_REGISTER_ZKBIN: &[u8] = include_bytes!("proof/rlnv2-diff-register.zk.bin");
pub const RLN2_SIGNAL_ZKBIN: &[u8] = include_bytes!("proof/rlnv2-diff-signal.zk.bin");
pub const RLN2_SLASH_ZKBIN: &[u8] = include_bytes!("proof/rlnv2-diff-slash.zk.bin");

pub const GENESIS_BLOB_GUARD: &[u8] = b"darkfi-rln-genesis-v1";
pub const GENESIS_USER_MSG_LIMIT: u64 = MAX_MSG_LIMIT;

/// RLN epoch genesis in millis.
/// Used as the time-zero reference for epoch numbering.
pub const RLN_GENESIS: u64 = 1_738_688_400_000;

/// Duration of one RLN epoch in millis (10 minutes).
pub const RLN_EPOCH_LEN: u64 = 600_000;

/// Network-wide cap on `user_message_limit`. Registrations must
/// pass this as the `max_message_limit` public input to the register
/// circuit, and verifiers must reject anything else.
pub const MAX_MSG_LIMIT: u64 = 100;

/// How many consecutive epochs of share metadata to keep around for
/// reuse detection. Must comfortably exceed [`crate::event_graph::EVENT_TIME_DRIFT`]
/// divided by [`RLN_EPOCH_LEN`] so that an honest event arriving late
/// across an epoch boundary still finds its sibling shares.
const METADATA_RETAIN_EPOCHS: u64 = 2;

/// Number of recent SMT roots to keep for signal proof verification.
/// Allows valid proofs created against a slightly stale tree to
/// still verify while registrations propagate across the network.
const ROOT_HISTORY_SIZE: usize = 16;

/// Wrapper for an RLN application identifier.
///
/// Per RLN-V1 Technical overview, this is a "random finite field
/// value unique per RLN app", used to prevent cross-app secret
/// correlation when the same identity credentials are reused across
/// different applications. It is mixed into the external nullifier
/// alongside the epoch.
///
/// In a multi-app deployment this should be derived from the app's
/// genesis (e.g. `poseidon_hash(genesis_contents_field)`), or
/// configured per app at startup. We expose it as a typed value so
/// it cannot be confused with an arbitrary field element.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct RlnAppId(pub pallas::Base);

impl RlnAppId {
    /// Derive a stable app identifier from the EventGraph's genesis
    /// contents. This way, two deployments using the same generic
    /// circuit but different genesis can never produce overlapping
    /// internal nullifiers.
    pub fn from_genesis(genesis_contents: &[u8]) -> Self {
        let mut buf = [0u8; 64];
        let h = blake3::hash(genesis_contents);
        buf[..32].copy_from_slice(h.as_bytes());
        Self(pallas::Base::from_uniform_bytes(&buf))
    }

    /// Construct from any field element. Useful for tests; in
    /// production prefer [`Self::from_genesis`] so the derivation
    /// is deterministic from the application's identity.
    pub fn from_field(v: pallas::Base) -> Self {
        Self(v)
    }

    pub fn as_field(&self) -> pallas::Base {
        self.0
    }
}

/// Versioned attestation accompanying a registration
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub enum RegistrationAttestation {
    /// No external attestation. The user_message_limit must be
    /// at most [`Self::FREE_TIER_LIMIT`].
    Free,
    SPECIAL,
    /// Reserved for the future staking integration.
    Staked(Vec<u8>),
}

impl RegistrationAttestation {
    /// In free-tier mode, hard cap on `user_message_limit`.
    pub const FREE_TIER_LIMIT: u64 = 10;
    /// In special-tier mode.
    pub const SPECIAL_TIER_LIMIT: u64 = 100;

    /// Validate the attestation against a claimed limit.
    pub fn permits(&self, user_message_limit: u64) -> bool {
        match self {
            Self::Free => user_message_limit <= Self::FREE_TIER_LIMIT,
            Self::SPECIAL => user_message_limit <= Self::SPECIAL_TIER_LIMIT,
            // Until staking is implemented, refuse to honor any
            // "Staked" attestation
            Self::Staked(_) => false,
        }
    }
}

/// The complete blob attached to a registration `EventPut` /
/// `StaticPut`. The proof's public inputs commit to the
/// `(commitment, user_message_limit, max_message_limit)` tuple,
/// and `attestation` carries the (eventual) staking proof.
#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct RegistrationBlob {
    pub proof: Proof,
    pub user_message_limit: u64,
    pub max_message_limit: u64,
    pub attestation: RegistrationAttestation,
}

/// The complete blob attached to a slashing `StaticPut`.
#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct SlashBlob {
    pub proof: Proof,
    /// The recovered identity_secret_hash.
    pub identity_secret_hash: pallas::Base,
    /// The SMT root the slash proof was constructed against.
    pub merkle_root: pallas::Base,
}

/// Ephemeral data attached to an [`EventPut`] when RLN is active.
#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct Blob {
    /// The RLN signal proof.
    pub proof: Proof,
    /// The `y` share value: `y = a_0 + x * a_1`.
    pub y: pallas::Base,
    /// Nullifier derived from `(identity, epoch, message_id)`.
    pub internal_nullifier: pallas::Base,
    /// The user's per-registration message limit.
    /// Now bound cryptographically as a public input to the signal
    /// proof, so the verifier can trust this value.
    pub user_msg_limit: u64,
    /// The SMT root the sender proved membership against.
    pub merkle_root: pallas::Base,
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
        ensure_key(sled_db, "rlnv2-diff-register-pk", RLN2_REGISTER_ZKBIN, KeyKind::Pk)?;
        ensure_key(sled_db, "rlnv2-diff-signal-vk", RLN2_SIGNAL_ZKBIN, KeyKind::Vk)?;
        ensure_key(sled_db, "rlnv2-diff-signal-pk", RLN2_SIGNAL_ZKBIN, KeyKind::Pk)?;
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
    pub fn load_slash_pk(&self) -> Result<ProvingKey> {
        read_pk(&self.sled_db, "rlnv2-diff-slash-pk", RLN2_SLASH_ZKBIN)
    }

    /// Load the register proving key from sled.
    pub fn load_register_pk(&self) -> Result<ProvingKey> {
        read_pk(&self.sled_db, "rlnv2-diff-register-pk", RLN2_REGISTER_ZKBIN)
    }

    /// Load the signal proving key from sled.
    pub fn load_signal_pk(&self) -> Result<ProvingKey> {
        read_pk(&self.sled_db, "rlnv2-diff-signal-pk", RLN2_SIGNAL_ZKBIN)
    }
}

/// Mutable RLN state shared across all protocol instances via
/// `EventGraph::rln_state`. Each peer connection's protocol handler
/// accesses this through a write lock so that duplicate/reuse
/// detection works regardless of which peer relayed the event.
pub struct RlnState {
    /// Per-nullifier share tracking, keyed first by epoch so we can
    /// prune by age rather than wiping on every epoch transition.
    pub metadata: MessageMetadata,
}

impl RlnState {
    pub fn new() -> Self {
        Self { metadata: MessageMetadata::new() }
    }
}

impl Default for RlnState {
    fn default() -> Self {
        Self::new()
    }
}

/// Outcome of [`EventGraph::rln_verify_signal`].
#[derive(Debug)]
pub enum SignalCheck {
    /// Proof valid, no conflict; the share has been recorded.
    Accepted,
    /// Drop silently. Covers: malformed blob, out-of-range
    /// `user_msg_limit`, unknown root, invalid proof, exact
    /// duplicate.
    Rejected,
    /// Different `(x, y)` for the same internal nullifier in this
    /// epoch - by SSS these expose `identity_secret_hash`. The
    /// caller should construct and broadcast a slash.
    ///
    /// When this variant is returned, the metadata table is *not*
    /// mutated. The conflicting share is included in the returned
    /// vector but not persisted, since the slash itself will remove
    /// the offending identity.
    Slashable(Vec<(pallas::Base, pallas::Base)>),
}

/// Outcome of [`EventGraph::rln_verify_static_event`].
///
/// Distinguishes "drop silently" (e.g. propagation race, unknown
/// root) from "the sender is malicious" (e.g. attestation didn't
/// permit the claimed limit, slash references the wrong commitment).
/// The protocol-layer wrapper translates `Malicious` into a strike
/// against the peer; tests can assert on the discriminator directly.
#[derive(Debug)]
pub enum StaticEventCheck {
    /// Registration verified; commitment should be inserted.
    AcceptedRegistration(pallas::Base),
    /// Slash verified; commitment should be removed.
    AcceptedSlash(pallas::Base),
    /// Drop silently (malformed blob, unknown root, duplicate
    /// commitment, invalid proof). Not strikable on its own
    /// because a peer might legitimately be relaying a stale
    /// event.
    Rejected,
    /// Sender is misbehaving and should be striked. Covers:
    /// out-of-range limits in registration, attestation that
    /// doesn't permit the claimed limit, slash whose recovered
    /// commitment doesn't match the claimed one.
    Malicious,
}

/// The set of currently registered RLN identities, stored as a Sparse
/// Merkle Tree (SMT).
///
/// Persistence model: leaf commitments are stored in a dedicated sled
/// tree (`rln-identity-leaves`). The in-memory SMT is rebuilt from
/// these leaves on startup.
pub struct IdentityState {
    smt: SmtMemoryFp,
    leaves: sled::Tree,
    recent_roots: VecDeque<pallas::Base>,
}

impl IdentityState {
    pub fn new(sled_db: &sled::Db) -> Result<Self> {
        let hasher = PoseidonFp::new();
        let store = MemoryStorageFp::new();
        let mut smt = SmtMemoryFp::new(store, hasher, &EMPTY_NODES_FP);

        let leaves = sled_db.open_tree("rln-identity-leaves")?;

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

        let mut recent_roots = VecDeque::with_capacity(ROOT_HISTORY_SIZE);
        recent_roots.push_back(smt.root());

        Ok(Self { smt, leaves, recent_roots })
    }

    /// Returns true if the commitment is already a leaf in the tree.
    pub fn contains(&self, commitment: &pallas::Base) -> bool {
        self.leaves.contains_key(commitment.to_repr()).unwrap_or(false)
    }

    /// Register a new identity.
    ///
    /// Returns `Err(Error::DuplicateIdentity)` if the commitment is
    /// already in the tree. Callers should treat this as a soft
    /// failure (drop the message), not as protocol-level malice.
    /// Concurrent honest registrations of the same commitment can
    /// race during P2P propagation.
    pub fn register(&mut self, commitment: pallas::Base) -> Result<()> {
        if self.contains(&commitment) {
            return Err(Error::Custom("RLN: duplicate identity commitment".into()))
        }
        self.leaves.insert(commitment.to_repr(), commitment.to_repr().as_ref())?;
        self.smt.insert_batch(vec![(commitment, commitment)])?;
        self.push_root();
        Ok(())
    }

    /// Slash (remove) an identity. Idempotent: removing a
    /// non-present commitment is a no-op rather than an error,
    /// because the same slash proof may legitimately arrive twice
    /// via different propagation paths.
    pub fn slash(&mut self, commitment: pallas::Base) -> Result<()> {
        if !self.contains(&commitment) {
            return Ok(())
        }
        self.leaves.remove(commitment.to_repr())?;
        self.smt.remove_leaves(vec![(commitment, commitment)])?;
        self.push_root();
        Ok(())
    }

    pub fn root(&self) -> pallas::Base {
        self.smt.root()
    }

    /// Number of leaves currently in the persistent identity tree.
    /// Used by [`EventGraph::rebuild_historical_roots_if_needed`] to
    /// detect a leaves-vs-events mismatch that bypasses the simpler
    /// recorded-count check (e.g. stale leaves left over from an
    /// older code path that bypassed `apply_rln_static_event`).
    pub(crate) fn leaves_count(&self) -> usize {
        self.leaves.len()
    }

    /// Check whether `root` matches the current root or any recent
    /// historical root. Used during signal proof verification to
    /// tolerate propagation delays.
    pub fn is_known_root(&self, root: &pallas::Base) -> bool {
        self.recent_roots.contains(root)
    }

    pub fn prove_membership(&self, commitment: &pallas::Base) -> darkfi_sdk::crypto::smt::PathFp {
        self.smt.prove_membership(commitment)
    }

    fn push_root(&mut self) {
        let root = self.smt.root();
        if self.recent_roots.len() >= ROOT_HISTORY_SIZE {
            self.recent_roots.pop_front();
        }
        self.recent_roots.push_back(root);
    }

    /// Reset the in-memory SMT and the persistent leaves tree to an
    /// empty state, in preparation for replaying the canonical
    /// static-DAG history.
    pub fn clear_for_rebuild(&mut self) -> Result<()> {
        // Drop every leaf from sled.
        self.leaves.clear()?;

        // Replace the in-memory SMT with a fresh empty one.
        let hasher = PoseidonFp::new();
        let store = MemoryStorageFp::new();
        self.smt = SmtMemoryFp::new(store, hasher, &EMPTY_NODES_FP);

        // Reset the recent-roots cache. The empty SMT root is the
        // current state.
        self.recent_roots.clear();
        self.recent_roots.push_back(self.smt.root());

        Ok(())
    }
}

/// Hash an event's header ID into a field element suitable for use
/// as the `x` coordinate in the RLN polynomial evaluation.
pub fn hash_event(event: &Event) -> pallas::Base {
    let mut buf = [0u8; 64];
    buf[..blake3::OUT_LEN].copy_from_slice(event.header.id().as_bytes());
    pallas::Base::from_uniform_bytes(&buf)
}

/// Map a UNIX-millis timestamp to its enclosing RLN epoch number.
///
/// Returns 0 if the timestamp predates [`RLN_GENESIS`], avoiding
/// underflow on malicious timestamps.
///
/// **Floor, not round.** The function returns the index of the
/// epoch that *contains* the given timestamp:
///
/// ```text
///     epoch N = [GENESIS + N * EPOCH_LEN, GENESIS + (N+1) * EPOCH_LEN)
/// ```
///
/// Floor-based assignment is essential for two reasons:
///
/// 1. **No ambiguity at boundaries.** A wall-clock instant always
///    belongs to exactly one epoch, regardless of the direction it
///    was approached from.
/// 2. **No partial overlap with the time-drift window.** The event
///    layer's `EVENT_TIME_DRIFT` is symmetric around `now`; epoch
///    rounding would create a window in which an honest message
///    could be "from the wrong epoch" relative to other peers.
///
/// Returns the epoch *number* (0, 1, 2, ...), not a timestamp, so
/// it is unambiguous and compact.
///
/// Use [`current_epoch`] when you want the epoch number at the
/// current wall-clock instant.
pub fn epoch_of(timestamp_millis: u64) -> u64 {
    let Some(diff) = timestamp_millis.checked_sub(RLN_GENESIS) else { return 0 };
    diff / RLN_EPOCH_LEN
}

/// The epoch number at the current wall-clock instant.
///
/// Convenience wrapper around [`epoch_of`] for the most common
/// call site. Use [`epoch_of`] explicitly when you need the epoch
/// for a *specific* timestamp (e.g. the timestamp of an event being
/// validated).
pub fn current_epoch() -> u64 {
    epoch_of(std::time::UNIX_EPOCH.elapsed().map(|d| d.as_millis() as u64).unwrap_or(0))
}

/// The wall-clock millis at the start of a given epoch number.
/// Inverse of [`epoch_of`].
pub fn epoch_start_millis(epoch: u64) -> u64 {
    RLN_GENESIS.saturating_add(epoch.saturating_mul(RLN_EPOCH_LEN))
}

#[derive(Debug, Clone, Default)]
struct ShareData {
    /// Collected `(x, y)` share pairs for a single internal nullifier.
    shares: Vec<(pallas::Base, pallas::Base)>,
}

/// Per-epoch tracking of RLN shares, keyed first by epoch number,
/// then by `internal_nullifier`.
///
/// We use `(epoch, internal_nullifier)` rather than
/// `(external_nullifier, internal_nullifier)`: the external nullifier
/// is `poseidon(epoch, app_id)` and the app_id is constant across all
/// shares we care about, so keying by epoch directly is equivalent
/// while making age-based pruning trivial.
///
/// This allows detecting:
/// * **Duplicates** - the exact same `(x, y)` pair arriving twice
///   (the event is just dropped).
/// * **Slot reuse** - a different `(x, y)` for the same internal
///   nullifier (the user reused a `message_id`, which by SSS reveals
///   their secret).
#[derive(Debug, Default)]
pub struct MessageMetadata {
    by_epoch: BTreeMap<u64, BTreeMap<pallas::Base, ShareData>>,
}

impl MessageMetadata {
    pub fn new() -> Self {
        Self::default()
    }

    /// Drop epochs older than `current_epoch - METADATA_RETAIN_EPOCHS`.
    /// Called opportunistically before/after each insert.
    pub fn prune_old(&mut self, current_epoch: u64) {
        let cutoff = current_epoch.saturating_sub(METADATA_RETAIN_EPOCHS);
        // BTreeMap::split_off keeps everything >= cutoff; everything
        // before cutoff is discarded.
        let keep = self.by_epoch.split_off(&cutoff);
        self.by_epoch = keep;
    }

    /// Record a new share.
    pub fn add_share(
        &mut self,
        epoch: u64,
        int_null: pallas::Base,
        x: pallas::Base,
        y: pallas::Base,
    ) {
        self.by_epoch.entry(epoch).or_default().entry(int_null).or_default().shares.push((x, y));
    }

    /// Retrieve all shares for a given (epoch, internal_nullifier).
    pub fn get_shares(
        &self,
        epoch: u64,
        int_null: &pallas::Base,
    ) -> Vec<(pallas::Base, pallas::Base)> {
        self.by_epoch
            .get(&epoch)
            .and_then(|m| m.get(int_null))
            .map(|sd| sd.shares.clone())
            .unwrap_or_default()
    }

    /// Check whether the exact `(x, y)` pair is already recorded.
    pub fn is_duplicate(
        &self,
        epoch: u64,
        int_null: &pallas::Base,
        x: &pallas::Base,
        y: &pallas::Base,
    ) -> bool {
        self.by_epoch
            .get(&epoch)
            .and_then(|m| m.get(int_null))
            .map(|sd| sd.shares.iter().any(|(sx, sy)| sx == x && sy == y))
            .unwrap_or(false)
    }

    /// Check whether any share has been recorded for this nullifier
    /// in the given epoch.
    ///
    /// In RLN-V2, each `message_id` produces a unique
    /// `internal_nullifier`. A repeated internal_nullifier therefore
    /// means the user reused the same `message_id` slot in the
    /// epoch, which is the V2 violation condition.
    ///
    /// (Note: the V1 spec phrased this in terms of "more than `limit`
    /// shares", but V2 changed the model - the rate limit itself is
    /// enforced inside the circuit by `message_id < user_message_limit`,
    /// so any repeat is by definition a violation.)
    pub fn is_reused(&self, epoch: u64, int_null: &pallas::Base) -> bool {
        self.by_epoch.get(&epoch).map(|m| m.contains_key(int_null)).unwrap_or(false)
    }
}

/// Create a ZK proof that a user's identity_secret_hash has been
/// recovered (via SSS) and they should be slashed from the identity tree.
///
/// `identity_secret_hash` here is the value the spec calls
/// "identity_secret_hash" - i.e. `poseidon(identity_secret, user_message_limit)`.
/// It is what SSS recovery actually returns from the updated signal
/// circuit, and from it the commitment is computable as
/// `poseidon(identity_secret_hash)` directly.
pub fn create_slash_proof(
    identity_secret_hash: pallas::Base,
    identity_state: &mut IdentityState,
    slash_pk: &ProvingKey,
) -> Result<(Proof, pallas::Base)> {
    let commitment = poseidon_hash([identity_secret_hash]);
    let root = identity_state.root();
    let path = identity_state.prove_membership(&commitment);

    let witnesses = vec![
        Witness::Base(Value::known(identity_secret_hash)),
        Witness::SparseMerklePath(Value::known(path.path)),
    ];
    let pi = vec![identity_secret_hash, root];
    let zkbin = ZkBinary::decode(RLN2_SLASH_ZKBIN, false)?;
    let circuit = ZkCircuit::new(witnesses, &zkbin);
    let proof = Proof::create(slash_pk, &[circuit], &pi, &mut OsRng)
        .map_err(|e| Error::Custom(format!("Slash proof creation failed: {e}")))?;
    Ok((proof, root))
}

/// Recover the secret from two or more `(x, y)` Shamir shares using
/// Lagrange interpolation.
///
/// What gets recovered is the constant term `a_0` of the polynomial.
/// In our updated signal circuit, that's the user's
/// `identity_secret_hash` (NOT the raw nullifier+trapdoor pair).
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

pub fn genesis_commitments() -> Vec<pallas::Base> {
    GENESIS_COMMITMENTS_REPR
        .iter()
        .filter_map(|repr| pallas::Base::from_repr(*repr).into())
        .collect()
}
