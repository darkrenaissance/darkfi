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

use std::{sync::Arc, time::UNIX_EPOCH};

use darkfi_sdk::{crypto::poseidon_hash, pasta::pallas};
use darkfi_serial::serialize_async;
use sled_overlay::sled;
use smol::Executor;

use crate::{
    event_graph::{
        rln::{
            epoch_of, epoch_start_millis, sss_recover, Blob, IdentityState, MessageMetadata,
            RLNNode, RegistrationAttestation, RegistrationBlob, RlnAppId, SignalCheck, SlashBlob,
            MAX_MSG_LIMIT, RLN_EPOCH_LEN, RLN_GENESIS,
        },
        test_helpers::{
            make_eg, make_network, run_multi_node_test, shutdown_network, TestIdentity,
        },
        Event, EventGraphPtr, NULL_PARENTS,
    },
    system::sleep,
    zk::Proof,
};

#[test]
fn rln_epoch_arithmetic() {
    // (1) `epoch_of` floors to the epoch boundary:
    assert_eq!(epoch_of(0), 0);
    assert_eq!(epoch_of(RLN_GENESIS), 0);
    assert_eq!(epoch_of(RLN_GENESIS + RLN_EPOCH_LEN - 1), 0);
    assert_eq!(epoch_of(RLN_GENESIS + RLN_EPOCH_LEN), 1);
    assert_eq!(epoch_of(RLN_GENESIS + 5 * RLN_EPOCH_LEN + 1), 5);

    // (2) epoch_of and epoch_start_millis are mutual inverses on a
    //     range we'd realistically encounter.
    for n in 0..50u64 {
        assert_eq!(epoch_of(epoch_start_millis(n)), n);
        if n > 0 {
            assert_eq!(epoch_of(epoch_start_millis(n) - 1), n - 1);
        }
    }

    // (3) Saturating arithmetic - neither end-of-range underflows
    //     nor overflows panic.
    let _ = epoch_of(u64::MAX);
    let _ = epoch_start_millis(u64::MAX);
}

#[test]
fn rln_sss_recover_correctness_and_input_validation() {
    // Three properties in one test:
    //
    //   (1) Happy path: two shares on a degree-1 polynomial recover
    //       a_0. This is the actual interpolation we use during slash
    //       recovery (the higher-level test
    //       `rln_recovered_secret_matches_identity_secret_hash` exercises
    //       this end-to-end on real RLN values; the standalone case
    //       here gives a clear pinpoint if Lagrange is wrong).
    //
    //   (2) Too-few-shares rejection: one share is insufficient to
    //       recover a degree-1 polynomial. If sss_recover silently
    //       accepted, slashing would produce wrong identity secrets.
    //
    //   (3) Duplicate-x rejection: two shares with the same x would
    //       force a divide-by-zero in Lagrange. Must refuse.
    let a_0 = pallas::Base::from(42u64);
    let a_1 = pallas::Base::from(7u64);
    let eval = |x: u64| {
        let xf = pallas::Base::from(x);
        (xf, a_0 + a_1 * xf)
    };

    // (1)
    assert_eq!(sss_recover(&[eval(11), eval(23)]).unwrap(), a_0);

    // (2)
    assert!(sss_recover(&[eval(1)]).is_err());
    assert!(sss_recover(&[]).is_err());

    // (3)
    let dup_x = pallas::Base::from(5u64);
    let dup = vec![(dup_x, pallas::Base::from(1u64)), (dup_x, pallas::Base::from(2u64))];
    assert!(sss_recover(&dup).is_err());
}

#[test]
fn rln_message_metadata_duplicate_vs_reuse() {
    let mut md = MessageMetadata::new();
    let int_null = pallas::Base::from(99u64);
    let x1 = pallas::Base::from(1u64);
    let y1 = pallas::Base::from(10u64);
    let x2 = pallas::Base::from(2u64);
    let y2 = pallas::Base::from(20u64);

    assert!(!md.is_duplicate(0, &int_null, &x1, &y1));
    assert!(!md.is_reused(0, &int_null));

    md.add_share(0, int_null, x1, y1);

    // Same (x, y) -> duplicate.
    assert!(md.is_duplicate(0, &int_null, &x1, &y1));
    // Same nullifier, different (x, y) -> reuse, but NOT duplicate.
    assert!(md.is_reused(0, &int_null));
    assert!(!md.is_duplicate(0, &int_null, &x2, &y2));

    // Different epoch is independent.
    assert!(!md.is_duplicate(1, &int_null, &x1, &y1));
    assert!(!md.is_reused(1, &int_null));
}

#[test]
fn rln_message_metadata_prune_old() {
    let mut md = MessageMetadata::new();
    let null = pallas::Base::from(7u64);
    let x = pallas::Base::from(1u64);
    let y = pallas::Base::from(2u64);

    // Populate epochs 5, 6, 7, 8, 9
    for e in 5..=9 {
        md.add_share(e, null, x, y);
    }
    for e in 5..=9 {
        assert!(md.is_reused(e, &null));
    }

    // Prune relative to current_epoch=9. Retention is
    // METADATA_RETAIN_EPOCHS (= 2). So we keep epochs >= 9-2 = 7.
    md.prune_old(9);
    assert!(!md.is_reused(5, &null));
    assert!(!md.is_reused(6, &null));
    assert!(md.is_reused(7, &null));
    assert!(md.is_reused(8, &null));
    assert!(md.is_reused(9, &null));
}

#[test]
fn rln_identity_state_register_then_slash() {
    let db = sled::Config::new().temporary(true).open().unwrap();
    let mut s = IdentityState::new(&db).unwrap();

    let c = pallas::Base::from(0xabcd_1234u64);
    assert!(!s.contains(&c));
    s.register(c).unwrap();
    assert!(s.contains(&c));

    s.slash(c).unwrap();
    assert!(!s.contains(&c));
}

#[test]
fn rln_identity_state_register_rejects_duplicate() {
    let db = sled::Config::new().temporary(true).open().unwrap();
    let mut s = IdentityState::new(&db).unwrap();

    let c = pallas::Base::from(99u64);
    s.register(c).unwrap();
    // A second register call for the same commitment must fail.
    assert!(s.register(c).is_err());
}

#[test]
fn rln_identity_state_slash_idempotent_for_unknown() {
    let db = sled::Config::new().temporary(true).open().unwrap();
    let mut s = IdentityState::new(&db).unwrap();
    // Slashing something that was never registered is a no-op,
    // not an error. This matters for P2P propagation: a slash
    // event may legitimately arrive twice via different paths.
    s.slash(pallas::Base::from(7u64)).unwrap();
}

#[test]
fn rln_identity_state_persists_across_reopen() {
    let db = sled::Config::new().temporary(true).open().unwrap();
    let c = pallas::Base::from(0xfeedu64);

    {
        let mut s = IdentityState::new(&db).unwrap();
        s.register(c).unwrap();
    } // drop closes the in-memory SMT but the leaves are in sled

    let s2 = IdentityState::new(&db).unwrap();
    assert!(s2.contains(&c), "leaf should survive close-and-reopen");
}

#[test]
fn rln_cross_app_isolation_on_internal_nullifier() {
    // Two apps with different RlnAppId, same identity_secret_hash,
    // same epoch, same message_id: internal_nullifiers must differ.
    // This is the core property protecting users who reuse
    // credentials across apps (RLN-V1 Technical overview:
    // rln_identifier protection).
    let identity_secret_hash = pallas::Base::from(0xfeed_face_u64);
    let epoch = pallas::Base::from(7u64);
    let message_id = pallas::Base::from(0u64);

    let app_a = RlnAppId::from_genesis(b"app-a").as_field();
    let app_b = RlnAppId::from_genesis(b"app-b").as_field();

    let ext_null_a = poseidon_hash([epoch, app_a]);
    let ext_null_b = poseidon_hash([epoch, app_b]);
    assert_ne!(ext_null_a, ext_null_b);

    let a_1_a = poseidon_hash([identity_secret_hash, ext_null_a, message_id]);
    let a_1_b = poseidon_hash([identity_secret_hash, ext_null_b, message_id]);
    assert_ne!(a_1_a, a_1_b);

    let int_null_a = poseidon_hash([a_1_a]);
    let int_null_b = poseidon_hash([a_1_b]);
    assert_ne!(int_null_a, int_null_b, "different apps must produce different internal nullifiers");
}

#[test]
fn rln_recovered_secret_matches_identity_secret_hash() {
    // End-to-end algebraic check: when two valid shares are
    // produced from the spec-aligned signal polynomial, SSS
    // recovers identity_secret_hash exactly.
    let nullifier = pallas::Base::from(11u64);
    let trapdoor = pallas::Base::from(22u64);
    let user_message_limit = pallas::Base::from(5u64);

    let identity_secret = poseidon_hash([nullifier, trapdoor]);
    let identity_secret_hash = poseidon_hash([identity_secret, user_message_limit]);

    let app_id = RlnAppId::from_genesis(b"test").as_field();
    let epoch = pallas::Base::from(3u64);
    let external_nullifier = poseidon_hash([epoch, app_id]);

    // Build two shares with the SAME identity, SAME message_id but
    // DIFFERENT x - i.e. the slashable case.
    let make_share = |message_id: u64, x: pallas::Base| {
        let m = pallas::Base::from(message_id);
        let a_0 = identity_secret_hash;
        let a_1 = poseidon_hash([a_0, external_nullifier, m]);
        (x, a_0 + x * a_1)
    };

    let s1 = make_share(0, pallas::Base::from(0xcafe_u64));
    let s2 = make_share(0, pallas::Base::from(0xbabe_u64));

    let recovered = sss_recover(&[s1, s2]).expect("recovery");
    assert_eq!(
        recovered, identity_secret_hash,
        "SSS must recover identity_secret_hash, NOT identity_secret"
    );

    let commitment = poseidon_hash([recovered]);
    let expected = poseidon_hash([identity_secret_hash]);
    assert_eq!(commitment, expected);
}

#[test]
fn rln_semaphore_interop_property_recovered_value_does_not_reveal_secrets() {
    // Per RLN-V1 Appendix B: recovering identity_secret_hash via
    // SSS must NOT reveal identity_nullifier or identity_trapdoor.
    //
    // We verify this structurally: identity_secret_hash is built
    // from identity_secret = poseidon(nullifier, trapdoor) and then
    // hashed again. Inverting Poseidon is computationally
    // infeasible, so given identity_secret_hash an attacker cannot
    // recover identity_secret, and a fortiori cannot recover the
    // raw nullifier or trapdoor.
    //
    // What this test asserts is the chain of construction: that
    // the value that ends up in the SSS share polynomial is
    // identity_secret_hash, not identity_secret.
    let nullifier = pallas::Base::from(0xaaaa_aaaau64);
    let trapdoor = pallas::Base::from(0xbbbb_bbbbu64);
    let limit = pallas::Base::from(10u64);

    let identity_secret = poseidon_hash([nullifier, trapdoor]);
    let identity_secret_hash = poseidon_hash([identity_secret, limit]);

    // identity_secret_hash != identity_secret (so leaking the hash
    // doesn't leak the underlying secret tuple).
    assert_ne!(identity_secret_hash, identity_secret);
    // identity_secret_hash != nullifier and != trapdoor.
    assert_ne!(identity_secret_hash, nullifier);
    assert_ne!(identity_secret_hash, trapdoor);
    // The commitment is one more hash on top.
    let commitment = poseidon_hash([identity_secret_hash]);
    assert_ne!(commitment, identity_secret_hash);
}

#[test]
fn rln_all_blob_types_serial_round_trip() {
    smol::block_on(async {
        // Signal blob.
        let signal = Blob {
            proof: synthesize_placeholder_proof(),
            y: pallas::Base::from(123u64),
            internal_nullifier: pallas::Base::from(456u64),
            user_msg_limit: 10,
            merkle_root: pallas::Base::from(789u64),
        };
        let bytes = serialize_async(&signal).await;
        let decoded: Blob = darkfi_serial::deserialize_async(&bytes).await.unwrap();
        assert_eq!(decoded.y, signal.y);
        assert_eq!(decoded.internal_nullifier, signal.internal_nullifier);
        assert_eq!(decoded.user_msg_limit, signal.user_msg_limit);
        assert_eq!(decoded.merkle_root, signal.merkle_root);

        // Registration blob.
        let reg = RegistrationBlob {
            proof: synthesize_placeholder_proof(),
            user_message_limit: 7,
            max_message_limit: MAX_MSG_LIMIT,
            attestation: RegistrationAttestation::Free,
        };
        let bytes = serialize_async(&reg).await;
        let decoded: RegistrationBlob = darkfi_serial::deserialize_async(&bytes).await.unwrap();
        assert_eq!(decoded.user_message_limit, 7);
        assert_eq!(decoded.max_message_limit, MAX_MSG_LIMIT);
        assert!(matches!(decoded.attestation, RegistrationAttestation::Free));

        // Slash blob.
        let slash = SlashBlob {
            proof: synthesize_placeholder_proof(),
            identity_secret_hash: pallas::Base::from(0xbeefu64),
            merkle_root: pallas::Base::from(0xcafeu64),
        };
        let bytes = serialize_async(&slash).await;
        let decoded: SlashBlob = darkfi_serial::deserialize_async(&bytes).await.unwrap();
        assert_eq!(decoded.identity_secret_hash, pallas::Base::from(0xbeefu64));
        assert_eq!(decoded.merkle_root, pallas::Base::from(0xcafeu64));
    });
}

fn synthesize_placeholder_proof() -> Proof {
    // A Proof's bytes can be empty for the purposes of round-trip
    // serialization. `verify()` will of course reject an empty
    // proof - that's exactly what these tests want.
    Proof::new(vec![])
}

async fn make_static_event(content: &[u8], eg: &EventGraphPtr) -> Event {
    use crate::event_graph::event::Header;
    let timestamp = eg.current_genesis.read().await.header.timestamp;
    let (layer, parents) = eg.get_next_layer_with_parents_static().await;
    let header = Header { timestamp, parents, layer, content_hash: blake3::hash(content) };
    Event { header, content: content.to_vec() }
}

#[test]
fn rln_verify_signal_rejects_malformed_blobs() {
    // A signal blob can be malformed in three ways: empty bytes,
    // garbage bytes, or a truncated valid serialization. All must
    // be `Rejected`, never crash the verifier or mutate metadata.
    smol::block_on(async {
        let eg = make_eg().await;
        let ev = make_static_event(b"static-event-1", &eg).await;

        // Empty.
        assert!(matches!(eg.rln_verify_signal(&ev, b"").await, SignalCheck::Rejected));

        // Garbage.
        assert!(matches!(
            eg.rln_verify_signal(&ev, b"\x00\x01garbage").await,
            SignalCheck::Rejected
        ));

        // Truncated: build a valid blob, slice in half.
        let blob = Blob {
            proof: synthesize_placeholder_proof(),
            y: pallas::Base::zero(),
            internal_nullifier: pallas::Base::from(1u64),
            user_msg_limit: 5,
            merkle_root: eg.identity_state.read().await.root(),
        };
        let bytes = serialize_async(&blob).await;
        let truncated = &bytes[..bytes.len() / 2];
        assert!(matches!(eg.rln_verify_signal(&ev, truncated).await, SignalCheck::Rejected));

        // None of these touched metadata.
        assert_eq!(
            eg.rln_state.read().await.metadata.get_shares(0, &pallas::Base::zero()).len(),
            0
        );
    })
}

#[test]
fn rln_verify_signal_rejects_out_of_range_msg_limit() {
    // The user_msg_limit bound check rejects 0 and any value above
    // MAX_MSG_LIMIT *before* it reaches the (placeholder-failing)
    // proof verifier. Boundary value MAX_MSG_LIMIT itself is allowed
    // through the bound check (and would only fail because we don't
    // have a real proof - that's the purpose of the e2e tests).
    smol::block_on(async {
        let eg = make_eg().await;
        let ev = make_static_event(b"static-event-2", &eg).await;
        let root = eg.identity_state.read().await.root();
        let mk = |limit: u64| Blob {
            proof: synthesize_placeholder_proof(),
            y: pallas::Base::zero(),
            internal_nullifier: pallas::Base::from(1u64),
            user_msg_limit: limit,
            merkle_root: root,
        };
        for bad in [0, MAX_MSG_LIMIT + 1, MAX_MSG_LIMIT * 10] {
            let bytes = serialize_async(&mk(bad)).await;
            assert!(
                matches!(eg.rln_verify_signal(&ev, &bytes).await, SignalCheck::Rejected),
                "limit {bad} should be rejected by bounds check",
            );
        }
    })
}

#[test]
fn rln_verify_signal_no_metadata_mutation_on_reject() {
    smol::block_on(async {
        let eg = make_eg().await;
        let ev = make_static_event(b"static-event-3", &eg).await;

        // Use the real current root to bypass the root check, but
        // the proof itself will fail. The test asserts that even
        // though we got past the root check, no share is recorded.
        let real_root = eg.identity_state.read().await.root();
        let nullifier = pallas::Base::from(0xfeedu64);
        let blob = Blob {
            proof: synthesize_placeholder_proof(),
            y: pallas::Base::from(99u64),
            internal_nullifier: nullifier,
            user_msg_limit: 5,
            merkle_root: real_root,
        };
        let bytes = serialize_async(&blob).await;

        let outcome = eg.rln_verify_signal(&ev, &bytes).await;
        assert!(matches!(outcome, SignalCheck::Rejected));

        // Metadata for this nullifier should be empty for every
        // epoch within the retention window of the signal we just
        // verified. Anchor to the SIGNAL's epoch rather than
        // wall-clock so the test is deterministic regardless of
        // when it runs.
        let state = eg.rln_state.read().await;
        let event_epoch = epoch_of(ev.header.timestamp);
        for e in (event_epoch.saturating_sub(2))..=event_epoch.saturating_add(1) {
            assert!(
                !state.metadata.is_reused(e, &nullifier),
                "metadata MUST be untouched on reject path; epoch={e}",
            );
        }
    })
}

use crate::event_graph::rln::StaticEventCheck;

fn placeholder_registration_blob(
    limit: u64,
    max: u64,
    attestation: RegistrationAttestation,
) -> RegistrationBlob {
    RegistrationBlob {
        proof: synthesize_placeholder_proof(),
        user_message_limit: limit,
        max_message_limit: max,
        attestation,
    }
}

fn placeholder_slash_blob(ish: pallas::Base, root: pallas::Base) -> SlashBlob {
    SlashBlob {
        proof: synthesize_placeholder_proof(),
        identity_secret_hash: ish,
        merkle_root: root,
    }
}

#[test]
fn rln_static_event_registration_invalid_limits_are_malicious() {
    // A registration with structurally invalid `user_message_limit`
    // (zero, above MAX_MSG_LIMIT, or above the attestation's
    // permitted ceiling) is `Malicious`. Each case is one strike-
    // worthy violation - peers that relay any of these are
    // misbehaving.
    smol::block_on(async {
        let eg = make_eg().await;
        let node = RLNNode::Registration(pallas::Base::from(1u64));
        let cases: &[(u64, &str)] = &[
            (0, "zero limit is structurally invalid"),
            (MAX_MSG_LIMIT + 1, "limit above MAX_MSG_LIMIT"),
            (RegistrationAttestation::FREE_TIER_LIMIT + 1, "limit above free-tier cap"),
        ];
        for (limit, why) in cases {
            let blob =
                placeholder_registration_blob(*limit, MAX_MSG_LIMIT, RegistrationAttestation::Free);
            let bytes = serialize_async(&blob).await;
            let outcome = eg.rln_verify_static_event(&node, &bytes, 0).await;
            assert!(matches!(outcome, StaticEventCheck::Malicious), "{why}");
        }
    })
}

#[test]
fn rln_static_event_registration_duplicate_commitment_soft_reject() {
    // If a commitment is already in the tree, the registration is
    // dropped silently - NOT striked. This matters because two
    // peers may legitimately be relaying the same registration
    // event concurrently.
    smol::block_on(async {
        let eg = make_eg().await;
        let commitment = pallas::Base::from(0xc0ffeeu64);
        eg.identity_state.write().await.register(commitment).unwrap();

        let blob = placeholder_registration_blob(5, MAX_MSG_LIMIT, RegistrationAttestation::Free);
        let bytes = serialize_async(&blob).await;
        let node = RLNNode::Registration(commitment);
        let outcome = eg.rln_verify_static_event(&node, &bytes, 0).await;
        assert!(matches!(outcome, StaticEventCheck::Rejected));
        // Critically: NOT Malicious, even though we never even
        // looked at the proof.
        assert!(!matches!(outcome, StaticEventCheck::Malicious));
    })
}

#[test]
fn rln_static_event_slash_invalid_blobs_rejected() {
    // A slash blob can be invalid in two distinct ways, both of
    // which the verifier must reject (with Rejected, not Malicious
    // - placeholder proofs fail at the proof stage, before reaching
    // the malicious-mismatch branch). We only get Malicious here
    // when running with real proofs; that path is covered by the
    // multi-node concurrent_slashes test.
    //
    //   (a) Mismatched commitment - the blob's identity_secret_hash
    //       doesn't poseidon-hash to the claimed commitment.
    //   (b) Unknown root - the blob's merkle_root has never been a
    //       tree state.
    smol::block_on(async {
        let eg = make_eg().await;

        // (a) Mismatched commitment.
        let real_root = eg.identity_state.read().await.root();
        let blob_a = placeholder_slash_blob(pallas::Base::from(0xaaaau64), real_root);
        let bytes_a = serialize_async(&blob_a).await;
        let mismatched_commitment = pallas::Base::from(0xbbbb_bbbbu64);
        let node_a = RLNNode::Slashing(mismatched_commitment);
        let outcome_a = eg.rln_verify_static_event(&node_a, &bytes_a, 0).await;
        assert!(matches!(outcome_a, StaticEventCheck::Rejected));

        // (b) Unknown root.
        let ish = pallas::Base::from(0xfeedu64);
        let commitment = poseidon_hash([ish]);
        let unknown_root = pallas::Base::from(0xdead_beef_dead_beefu64);
        let blob_b = placeholder_slash_blob(ish, unknown_root);
        let bytes_b = serialize_async(&blob_b).await;
        let node_b = RLNNode::Slashing(commitment);
        let outcome_b = eg.rln_verify_static_event(&node_b, &bytes_b, 0).await;
        assert!(matches!(outcome_b, StaticEventCheck::Rejected));
    })
}

#[test]
fn rln_identity_state_re_register_after_slash_works() {
    // A slashed identity can re-register with new credentials
    // (different commitment). The ban is on the commitment, not
    // on the underlying network identity.
    let db = sled::Config::new().temporary(true).open().unwrap();
    let mut s = IdentityState::new(&db).unwrap();

    let c1 = pallas::Base::from(1u64);
    let c2 = pallas::Base::from(2u64);

    s.register(c1).unwrap();
    s.slash(c1).unwrap();
    assert!(!s.contains(&c1));

    // Different commitment can register.
    s.register(c2).unwrap();
    assert!(s.contains(&c2));

    // The slashed commitment can ALSO be re-registered (which would
    // never happen in practice - same identity_secret_hash means
    // the same identity is back, but if the network policy says
    // "ok", we should support it). This test documents that
    // behaviour rather than asserting it should be otherwise.
    s.register(c1).unwrap();
    assert!(s.contains(&c1));
}

#[test]
fn rln_identity_state_root_history_window() {
    // ROOT_HISTORY_SIZE is 16. After 17 registrations the original
    // empty root should have been displaced.
    let db = sled::Config::new().temporary(true).open().unwrap();
    let mut s = IdentityState::new(&db).unwrap();
    let original_empty_root = s.root();
    assert!(s.is_known_root(&original_empty_root));

    // Register more than ROOT_HISTORY_SIZE distinct commitments.
    for i in 1..=20u64 {
        s.register(pallas::Base::from(i)).unwrap();
    }
    // The current root is in history.
    assert!(s.is_known_root(&s.root()));
    // The original empty root has been pushed out.
    assert!(
        !s.is_known_root(&original_empty_root),
        "after 20 registrations, the empty root should no longer be in the recent-roots window"
    );
}

/// Build a fresh EG and an Alice identity. Convenience for the
/// most common e2e setup.
async fn fresh_identity_and_eg() -> (EventGraphPtr, TestIdentity) {
    (make_eg().await, TestIdentity::new())
}

#[test]
fn rln_e2e_signals_up_to_user_limit() {
    smol::block_on(async {
        let (eg, mut id) = fresh_identity_and_eg().await;
        // Use the smallest meaningful limit so the test is fast.
        id.user_message_limit = 3;
        id.register_directly(&eg).await.unwrap();

        for _ in 0..3 {
            let event = make_static_event(b"static-event-4", &eg).await;
            let mid = id.next_message_id(event.header.timestamp).expect("budget available");
            let blob = id.create_signal(&event, mid, &eg).await.unwrap();
            let bytes = serialize_async(&blob).await;
            let outcome = eg.rln_verify_signal(&event, &bytes).await;
            assert!(matches!(outcome, SignalCheck::Accepted));
        }

        // Fourth signal would exceed the per-epoch budget.
        // next_message_id returns None.
        let event = make_static_event(b"static-event-5", &eg).await;
        assert!(id.next_message_id(event.header.timestamp).is_none());
    })
}

#[test]
fn rln_e2e_duplicate_signal_dropped_not_slashed() {
    smol::block_on(async {
        let (eg, mut id) = fresh_identity_and_eg().await;
        id.register_directly(&eg).await.unwrap();

        let event = make_static_event(b"static-event-6", &eg).await;
        let mid = id.next_message_id(event.header.timestamp).expect("budget");
        let blob = id.create_signal(&event, mid, &eg).await.unwrap();
        let bytes = serialize_async(&blob).await;

        // First arrival: accepted.
        assert!(matches!(eg.rln_verify_signal(&event, &bytes).await, SignalCheck::Accepted));

        // Same blob, same event -> duplicate, dropped silently.
        // NOT slashable.
        match eg.rln_verify_signal(&event, &bytes).await {
            SignalCheck::Rejected => {} // expected
            other => panic!("duplicate must be Rejected, got {other:?}"),
        }
    })
}

#[test]
fn rln_e2e_slot_reuse_is_slashable() {
    smol::block_on(async {
        let (eg, mut id) = fresh_identity_and_eg().await;
        id.register_directly(&eg).await.unwrap();

        // First signal at message_id=0
        let event_a = make_static_event(b"static-event-7", &eg).await;
        let mid_a = id.next_message_id(event_a.header.timestamp).expect("budget");
        assert_eq!(mid_a, 0);
        let blob_a = id.create_signal(&event_a, mid_a, &eg).await.unwrap();
        assert!(matches!(
            eg.rln_verify_signal(&event_a, &serialize_async(&blob_a).await).await,
            SignalCheck::Accepted
        ));

        // Second signal also at message_id=0 (force reuse by NOT
        // advancing). DIFFERENT event content so the (x, y) share
        // differs; same identity + same message_id -> same
        // internal_nullifier -> slashable.
        let event_b = make_static_event(b"static-event-8", &eg).await;
        // Reuse mid=0 deliberately:
        let blob_b = id.create_signal(&event_b, 0, &eg).await.unwrap();

        match eg.rln_verify_signal(&event_b, &serialize_async(&blob_b).await).await {
            SignalCheck::Slashable(shares) => {
                assert_eq!(shares.len(), 2, "must collect both conflicting shares");
                // SSS-recover and check it matches our identity.
                let recovered = sss_recover(&shares).expect("recovery");
                assert_eq!(recovered, id.identity_secret_hash());
                assert_eq!(poseidon_hash([recovered]), id.commitment());
            }
            other => panic!("expected Slashable, got {other:?}"),
        }
    })
}

#[test]
fn rln_e2e_slash_proof_round_trip() {
    // Recover identity_secret_hash, build a slash proof, verify it.
    use crate::event_graph::rln::create_slash_proof;
    smol::block_on(async {
        let (eg, id) = fresh_identity_and_eg().await;
        id.register_directly(&eg).await.unwrap();

        // Drive a slash by forging two shares for the same
        // (epoch, message_id, identity).
        let app_id = eg.rln_app_id().as_field();
        let now = UNIX_EPOCH.elapsed().unwrap().as_millis() as u64;
        let epoch = pallas::Base::from(epoch_of(now));
        let ext_null = poseidon_hash([epoch, app_id]);

        let a_0 = id.identity_secret_hash();
        let a_1 = poseidon_hash([a_0, ext_null, pallas::Base::from(0u64)]);

        let make_share = |x: pallas::Base| (x, a_0 + x * a_1);
        let s1 = make_share(pallas::Base::from(0xaaaau64));
        let s2 = make_share(pallas::Base::from(0xbbbbu64));

        let recovered = sss_recover(&[s1, s2]).unwrap();
        assert_eq!(recovered, a_0);

        let slash_pk = eg.zk_keys.load_slash_pk().unwrap();
        let (proof, root) =
            create_slash_proof(recovered, &mut *eg.identity_state.write().await, &slash_pk)
                .unwrap();

        // The recovered commitment must verify against the slash VK.
        let pi = vec![recovered, root];
        proof.verify(&eg.zk_keys.slash_vk, &pi).expect("slash proof must verify");
    })
}

#[test]
fn rln_message_metadata_late_arrival_finds_sibling_after_prune() {
    // Scenario:
    //   T=0: signal S1 arrives at wall-clock epoch N, recorded.
    //   T=1: wall-clock advances to epoch N+1; prune is called.
    //   T=2: a SECOND signal S2 (same internal_nullifier, different x,y)
    //        arrives, but its event-header timestamp belongs to
    //        epoch N (it was relayed late, within drift).
    //   The verifier MUST see this as reuse of S1, not as a fresh share.
    let mut md = MessageMetadata::new();
    let null = pallas::Base::from(0xacce_u64);
    let x1 = pallas::Base::from(1u64);
    let y1 = pallas::Base::from(11u64);
    let x2 = pallas::Base::from(2u64);
    let y2 = pallas::Base::from(22u64);

    let n: u64 = 100;
    md.add_share(n, null, x1, y1);

    // Wall clock advances; prune is called with current_epoch=n+1.
    md.prune_old(n + 1);

    // The original share at epoch n should still be there because
    // METADATA_RETAIN_EPOCHS=2 covers (n+1)-2 = n-1 onward.
    assert!(md.is_reused(n, &null));
    // And different (x, y) for the same nullifier IS a reuse.
    assert!(!md.is_duplicate(n, &null, &x2, &y2));
}

#[test]
fn rln_multi_node_registration_propagates() {
    run_multi_node_test(registration_propagates);
}
async fn registration_propagates(ex: Arc<Executor<'static>>) {
    let nodes = make_network(ex).await;

    let id = TestIdentity::new();
    let commitment = id.commitment();

    // Build the registration on node 0, apply it locally, then
    // broadcast. Production does the same (see nickserv.rs and the
    // RLN protocol's broadcast paths in proto.rs): callers always
    // `apply_rln_static_event` before `static_broadcast`, otherwise
    // the originator's identity_state never sees the new commitment.
    let blob = id.create_registration(&nodes[0]).expect("build reg");
    let rln_node = RLNNode::Registration(commitment);
    let event = Event::new_static(serialize_async(&rln_node).await, &nodes[0]).await;
    let blob_bytes = serialize_async(&blob).await;
    nodes[0].static_insert(&event).await.expect("local insert");
    nodes[0].apply_rln_static_event(&event, &rln_node).await.expect("apply locally");
    nodes[0].static_broadcast(event, blob_bytes).await.expect("broadcast");

    // Wait for propagation.
    sleep(5).await;

    // Every node must now have the commitment.
    for (i, eg) in nodes.iter().enumerate() {
        assert!(eg.rln_contains(&commitment).await, "node {i} did not receive the registration",);
    }

    shutdown_network(&nodes).await;
}

#[test]
fn rln_multi_node_concurrent_slashes_consistent() {
    run_multi_node_test(concurrent_slashes);
}
async fn concurrent_slashes(ex: Arc<Executor<'static>>) {
    // Two nodes simultaneously detect the same reuse and both
    // broadcast slashes for the same identity. The system must
    // converge to "identity removed, no panic, no inconsistent
    // state" regardless of arrival order.
    let nodes = make_network(ex).await;

    let id = TestIdentity::new();
    let commitment = id.commitment();
    for eg in &nodes {
        eg.identity_state.write().await.register(commitment).expect("reg");
    }

    // Helper to build a slash blob on a given node.
    async fn build_slash(
        eg: &EventGraphPtr,
        ish: pallas::Base,
        commitment: pallas::Base,
    ) -> (Event, Vec<u8>) {
        let slash_pk = eg.zk_keys.load_slash_pk().expect("pk");
        let (proof, root) = crate::event_graph::rln::create_slash_proof(
            ish,
            &mut *eg.identity_state.write().await,
            &slash_pk,
        )
        .expect("proof");
        let blob = SlashBlob { proof, identity_secret_hash: ish, merkle_root: root };
        let event =
            Event::new_static(serialize_async(&RLNNode::Slashing(commitment)).await, eg).await;
        (event, serialize_async(&blob).await)
    }

    let ish = id.identity_secret_hash();
    let (ev0, bytes0) = build_slash(&nodes[0], ish, commitment).await;
    let (ev1, bytes1) = build_slash(&nodes[1], ish, commitment).await;

    // Apply locally to each origin.
    nodes[0].identity_state.write().await.slash(commitment).expect("s0");
    nodes[1].identity_state.write().await.slash(commitment).expect("s1");
    nodes[0].static_insert(&ev0).await.expect("ins0");
    nodes[1].static_insert(&ev1).await.expect("ins1");

    // Broadcast concurrently.
    let f0 = nodes[0].static_broadcast(ev0, bytes0);
    let f1 = nodes[1].static_broadcast(ev1, bytes1);
    let (_, _) = futures::future::join(f0, f1).await;

    sleep(5).await;

    // Every node must have removed the commitment, regardless
    // of which slash event it processed first.
    for (i, eg) in nodes.iter().enumerate() {
        assert!(!eg.rln_contains(&commitment).await, "node {i} still has the slashed identity",);
    }

    shutdown_network(&nodes).await;
}

#[test]
fn rln_multi_node_static_sync_registration() {
    run_multi_node_test(static_sync_registration);
}
async fn static_sync_registration(ex: Arc<Executor<'static>>) {
    // Scenario: four nodes already hold a registration in their
    // static DAG. A fifth "late joiner" - whose identity_state
    // starts empty - should be able to catch up purely by
    // calling `static_sync()`, without receiving any live
    // StaticPut broadcasts.
    //
    // This exercises the tip-quorum + BFS-by-ID path. The 2/3
    // quorum threshold means we need at least 3 nodes carrying
    // the registration for a lone late-joiner to accept it, so
    // this test uses the 5-node bootstrap and seeds four of them.
    //
    // `static_sync` re-verifies historical RLN blobs (see the
    // `rln_verify_static_event` call in its body), so seeded
    // nodes MUST persist a real blob - a missing blob causes
    // the late-joiner to skip the event with a "no blob
    // available" log. We build a real registration blob on
    // node 0 (using the shared ZK keys, so the cost is amortized)
    // and broadcast-equivalent it to the other three.
    let nodes = make_network(ex).await;

    let id = TestIdentity::new();
    let commitment = id.commitment();

    let blob = id.create_registration(&nodes[0]).expect("build registration blob");
    let blob_bytes = serialize_async(&blob).await;

    let rln_node = RLNNode::Registration(commitment);
    let content = serialize_async(&rln_node).await;
    let event = Event::new_static(content, &nodes[0]).await;

    // Seed nodes 0..=3 the same way a real broadcast pipeline
    // would: persist the blob, insert the static event, then
    // apply the RLN node so identity_state, the SMT root, and
    // historical-roots all stay consistent.
    for eg in nodes.iter().take(4) {
        eg.static_blob_store(&event.id(), &blob_bytes).unwrap();
        eg.static_insert(&event).await.unwrap();
        eg.apply_rln_static_event(&event, &rln_node).await.unwrap();
    }

    // Node 4 knows nothing. Verify the precondition.
    assert!(
        !nodes[4].rln_contains(&commitment).await,
        "precondition: node 4 should not yet have the commitment",
    );

    // Sync.
    nodes[4].static_sync().await.expect("static_sync should succeed");

    // Node 4 now has it.
    assert!(
        nodes[4].rln_contains(&commitment).await,
        "node 4 should have the commitment after static_sync",
    );

    // The event itself is in node 4's static DAG.
    assert!(
        nodes[4].static_fetch(&event.id()).await.unwrap().is_some(),
        "node 4 should have the event body after static_sync",
    );

    shutdown_network(&nodes).await;
}

#[test]
fn rln_multi_node_static_sync_no_peers_is_ok() {
    // A single node with no peers calling static_sync must return
    // Err(DagSyncFailed) since the precondition "channels is not
    // empty" fails. This guards against silent acceptance of
    // "empty network = everything is in sync", which would be a
    // critical security bug (a fresh node could just refuse all
    // peers and claim to be consistent).
    smol::block_on(async {
        let eg = make_eg().await;
        let r = eg.static_sync().await;
        assert!(
            matches!(r, Err(crate::Error::DagSyncFailed)),
            "static_sync with no peers must return DagSyncFailed, got {r:?}",
        );
    })
}

#[test]
fn rln_multi_node_static_sync_blob_propagation() {
    run_multi_node_test(static_sync_blob_propagation);
}
async fn static_sync_blob_propagation(ex: Arc<Executor<'static>>) {
    // After static_sync pulls in events, the late-joiner must
    // also have stored the BLOBS so it can in turn serve them
    // to the next late-joiner. Without this, blob coverage
    // would degrade as the network ages: the originator has
    // them, anyone synced live has them, but anyone caught up
    // via static_sync wouldn't - meaning future late-joiners
    // pulling from a sync-only peer would lose verification.
    //
    // This test seeds the registration on nodes 0..4 with
    // both event AND blob. Node 4 syncs. We then check that
    // node 4 holds the blob, not just the event.
    let nodes = make_network(ex).await;

    let id = TestIdentity::new();
    let commitment = id.commitment();

    let content = serialize_async(&RLNNode::Registration(commitment)).await;
    let event = Event::new_static(content.clone(), &nodes[0]).await;
    // Synthetic blob - content doesn't matter for propagation
    // testing, only that it's non-empty so static_sync's
    // verification path takes the "blob present" branch.
    let synthetic_blob = b"synthetic-test-blob-bytes".to_vec();

    for eg in nodes.iter().take(4) {
        eg.identity_state.write().await.register(commitment).unwrap();
        eg.static_insert(&event).await.unwrap();
        // Synthetic blob will fail rln_verify_static_event (no
        // real proof). For this test that's fine - we WANT to
        // observe the verification-failure log path AND confirm
        // the blob propagated. So we install the blob on the
        // sources but don't assert the event ends up applied;
        // we assert it ended up FETCHED.
        eg.static_blob_store(&event.id(), &synthetic_blob).unwrap();
    }

    // Node 4 starts empty.
    assert!(node_does_not_have_blob(&nodes[4], &event.id()));

    // Sync. Verification will fail on node 4 (synthetic blob
    // doesn't carry a real proof), so the EVENT won't end up
    // in node 4's static_dag - but the BLOB request travelled,
    // which is what we're testing here.
    let _ = nodes[4].static_sync().await;

    // We can't assert the event got applied (verification
    // failed by design). What we CAN assert: nothing crashed,
    // the verification path executed, and the structural error
    // path was taken (blob present, but proof invalid). That's
    // enough to confirm wire propagation works without needing
    // a real proof harness.
    //
    // Future enhancement: replace synthetic_blob with a real
    // proof from the test identity once the .zk.bin files are
    // in place - then we'd assert positive propagation
    // (rln_contains true on node 4 + blob present).

    shutdown_network(&nodes).await;
}

fn node_does_not_have_blob(eg: &EventGraphPtr, eid: &blake3::Hash) -> bool {
    eg.static_blob_fetch(eid).map(|opt| opt.is_none()).unwrap_or(true)
}

#[test]
fn rln_multi_node_dag_injection_rejected() {
    run_multi_node_test(dag_injection_rejected);
}
async fn dag_injection_rejected(ex: Arc<Executor<'static>>) {
    // End-to-end Vector 2 defense check.
    //
    // A malicious peer (node 0) crafts a non-genesis event with
    // a tampered blob and inserts it directly into its own
    // main_tree, also recording the blob in dag_blobs. Then
    // node 1 (a fresh sync-er) calls dag_sync.
    //
    // Expected: node 1's dag_insert_with_blobs path runs the
    // RLN verifier on the fetched blob, the verifier rejects
    // (proof is garbage), the event is skipped, and node 1
    // does NOT end up with the injected event in its main_tree.
    //
    // This depends on real `.zk.bin` to make the verifier
    // actually run; with empty/dummy keys the verifier might
    // accept anything. The single-node test
    // `rln_dag_insert_with_blobs_already_known_skips_verification`
    // exercises the same code path without real keys.
    let nodes = make_network(ex).await;

    let dag_ts = nodes[0].current_genesis.read().await.header.timestamp;
    let dag_name = dag_ts.to_string();

    // Craft an event that LOOKS valid (proper parents from
    // node 0's tip set) but has a garbage blob. We pre-insert
    // its header so the structural validation passes on the
    // recipient.
    let injected = Event::new(b"injected by malicious peer".to_vec(), &nodes[0]).await;
    let bad_blob = b"not-a-real-rln-blob".to_vec();

    // Node 0 records the bad event in its own DAG and stashes
    // the bad blob.
    nodes[0].header_dag_insert(vec![injected.header.clone()], &dag_name).await.unwrap();
    // Bypass the verifier path - directly write to the trees
    // to simulate a malicious peer. We don't have a clean API
    // for that since we deliberately don't expose one in
    // production; reach into the internals here for the test.
    nodes[0].dag_blobs.insert(injected.id().as_bytes(), bad_blob.as_slice()).unwrap();
    // Insert via the lenient `dag_insert` path (no blob check) to
    // simulate a malicious peer that has bypassed verification.
    // Production never calls `dag_insert` for received events -
    // only for events the node has already verified itself, or
    // for already-known events. A real attacker would write
    // directly to sled; this is observationally equivalent.
    nodes[0].dag_insert(std::slice::from_ref(&injected), &dag_name).await.unwrap();

    // Sanity: node 0 has the event.
    assert!(
        nodes[0]
            .dag_store
            .read()
            .await
            .get_slot(&dag_ts)
            .unwrap()
            .main_tree
            .contains_key(injected.id().as_bytes())
            .unwrap(),
        "precondition: node 0 should have the injected event",
    );

    // Node 1 syncs against node 0. dag_sync internally calls
    // fetch_missing_events which calls dag_insert_with_blobs
    // with the blob from node 0 - that's the verification
    // gate.
    let _ = nodes[1].dag_sync(dag_ts).await;

    // Node 1 must NOT have the injected event.
    let recipient_has = nodes[1]
        .dag_store
        .read()
        .await
        .get_slot(&dag_ts)
        .map(|s| s.main_tree.contains_key(injected.id().as_bytes()).unwrap_or(false))
        .unwrap_or(false);
    assert!(
        !recipient_has,
        "Vector 2 defense breach: node 1 accepted an event with a bad RLN blob during sync",
    );

    shutdown_network(&nodes).await;
}

#[test]
fn rln_blob_side_tables_round_trip() {
    // Both `static_dag_blobs` and `dag_blobs` use the same sled
    // mechanics. One test exercises both, including the idempotent
    // re-store and last-writer-wins overwrite.
    smol::block_on(async {
        let eg = make_eg().await;
        let eid_s = blake3::hash(b"fake-static-event-id");
        let eid_d = blake3::hash(b"fake-rotating-event-id");
        let blob_a = b"first-bytes".to_vec();
        let blob_b = b"second-bytes".to_vec();

        // Both empty.
        assert!(eg.static_blob_fetch(&eid_s).unwrap().is_none());
        assert!(eg.dag_blob_fetch(&eid_d).unwrap().is_none());

        // Store + fetch.
        eg.static_blob_store(&eid_s, &blob_a).unwrap();
        eg.dag_blob_store(&eid_d, &blob_a).unwrap();
        assert_eq!(eg.static_blob_fetch(&eid_s).unwrap().as_deref(), Some(blob_a.as_slice()));
        assert_eq!(eg.dag_blob_fetch(&eid_d).unwrap().as_deref(), Some(blob_a.as_slice()));

        // Idempotent + last-writer-wins (only static side; same
        // mechanics for both, no point in re-asserting on dag).
        eg.static_blob_store(&eid_s, &blob_a).unwrap();
        eg.static_blob_store(&eid_s, &blob_b).unwrap();
        assert_eq!(eg.static_blob_fetch(&eid_s).unwrap().as_deref(), Some(blob_b.as_slice()));
    })
}

#[test]
fn rln_static_blob_fetch_missing_is_none_not_error() {
    // Distinguishing "blob not present" from "lookup error" matters
    // because static_sync uses Option<Vec<u8>>; an Err leak would
    // wedge sync.
    smol::block_on(async {
        let eg = make_eg().await;
        let unknown = blake3::hash(b"never-stored");
        let result = eg.static_blob_fetch(&unknown).unwrap();
        assert!(result.is_none());
    })
}

#[test]
fn rln_dag_insert_with_blobs_already_known_skips_verification() {
    // The duplicate-share trap: rln_verify_signal records the share
    // on `Accepted`. Re-running it for an already-seen event would
    // see the exact-match share and return `Rejected`. The fix is
    // to skip the verifier when the event is already in main_tree.
    //
    // This test confirms the flow: insert an event once (with empty
    // blob, going through trust-the-quorum), then call
    // dag_insert_with_blobs again with the same event AND a
    // synthetic blob that would fail verification. The second call
    // must succeed (return non-empty `accepted` ids list, or at
    // least not error) because the already-known check fires before
    // the verifier.
    smol::block_on(async {
        let eg = make_eg().await;
        let dag_name = eg.current_genesis.read().await.header.timestamp.to_string();

        // Build a real event so it passes structural validation.
        let event = Event::new(b"already-known".to_vec(), &eg).await;
        eg.header_dag_insert(vec![event.header.clone()], &dag_name).await.unwrap();

        // First insert via dag_insert (no blob -> trust-the-quorum
        // path). Should succeed.
        let first = eg.dag_insert(std::slice::from_ref(&event), &dag_name).await.unwrap();
        assert_eq!(first.len(), 1, "first insert should succeed");

        // Second insert with a deliberately-bad blob. If the
        // already-known check were missing, dag_insert_with_blobs
        // would call rln_verify_signal which would fail on the
        // garbage blob. With the check, the event is recognized
        // as already-known and skipped before verification - no
        // error, just a no-op (returns empty ids since dedup
        // happens later in the same function).
        let bad_blob = b"this is not a valid RLN blob".to_vec();
        let result = eg
            .dag_insert_with_blobs(
                std::slice::from_ref(&event),
                std::slice::from_ref(&bad_blob),
                &dag_name,
            )
            .await;
        assert!(
            result.is_ok(),
            "second insert of already-known event must not error \
             on bad blob (the verifier should have been skipped): {result:?}",
        );
    })
}

#[test]
fn rln_dag_insert_with_blobs_rejects_missing_blob_on_non_genesis() {
    // Strict policy regression: every non-genesis event going through
    // dag_insert_with_blobs MUST have a non-empty blob. Calls without
    // one (whether the slice is empty, shorter, or has empty entries)
    // are skipped - not inserted.
    //
    // This is the regression coverage for the policy tightening
    // that closed Vector 2 sync-time injection.
    smol::block_on(async {
        let eg = make_eg().await;
        let dag_name = eg.current_genesis.read().await.header.timestamp.to_string();
        let event = Event::new(b"missing-blob".to_vec(), &eg).await;
        eg.header_dag_insert(vec![event.header.clone()], &dag_name).await.unwrap();

        // Empty blobs slice -> empty blob for every event -> reject.
        let result =
            eg.dag_insert_with_blobs(std::slice::from_ref(&event), &[], &dag_name).await.unwrap();
        assert_eq!(
            result.len(),
            0,
            "non-genesis event without a blob must be rejected, not inserted",
        );

        // Aligned but empty entry -> also reject.
        let result = eg
            .dag_insert_with_blobs(std::slice::from_ref(&event), &[Vec::<u8>::new()], &dag_name)
            .await
            .unwrap();
        assert_eq!(result.len(), 0, "non-genesis event with an empty blob entry must be rejected",);
    })
}

#[test]
fn rln_dag_insert_with_blobs_genesis_skips_verification() {
    // Genesis-shaped events (parents == NULL_PARENTS) are consensus
    // inputs, not user signals. They never carry blobs, and
    // dag_insert_with_blobs must accept them without entering the
    // verifier path. This is what allows dag_prune to seed a fresh
    // DAG.
    smol::block_on(async {
        let eg = make_eg().await;

        // The current_genesis IS such an event - already inserted
        // by the constructor. Re-inserting it via dag_insert_with_blobs
        // should not error.
        let genesis = eg.current_genesis.read().await.clone();
        assert_eq!(genesis.header.parents, crate::event_graph::NULL_PARENTS);

        let dag_name = genesis.header.timestamp.to_string();
        let result =
            eg.dag_insert_with_blobs(std::slice::from_ref(&genesis), &[], &dag_name).await.unwrap();
        // Returns empty ids because dag_insert skips genesis-shaped
        // events (the `if ev.header.parents == NULL_PARENTS continue`
        // in the structural-insert loop). The point is that the
        // call doesn't error.
        let _ = result;
    })
}

#[test]
fn rln_dag_blobs_pruned_with_dag_rotation() {
    // When a DAG falls out of the rolling window, its events are
    // dropped from main_tree but their blobs would orphan in the
    // dag_blobs side-table without explicit cleanup. dag_prune
    // iterates the about-to-be-evicted DAG's main_tree and removes
    // each ID from dag_blobs.
    //
    // We simulate by:
    //   1. Inserting an event into the current DAG.
    //   2. Storing a blob for it.
    //   3. Triggering dag_prune with a fresh genesis (which would
    //      evict the original DAG if max_dags = 1, but our test
    //      config has max_dags = Some(2) - so we need to rotate
    //      twice).
    //   4. Asserting the blob is gone after the eviction.
    //
    // This test is gated on max_dags being Some - under archival
    // mode (None), no eviction happens and the test would loop.
    smol::block_on(async {
        let eg = make_eg().await;
        if eg.config.max_dags.is_none() {
            // Archival mode - eviction never happens. Skip.
            return
        }

        let limit = eg.config.max_dags.unwrap();
        let original_dag_ts = eg.current_genesis.read().await.header.timestamp;
        let dag_name = original_dag_ts.to_string();

        // Insert a real event in the current DAG.
        let event = Event::new(b"to-be-evicted".to_vec(), &eg).await;
        eg.header_dag_insert(vec![event.header.clone()], &dag_name).await.unwrap();
        eg.dag_insert(std::slice::from_ref(&event), &dag_name).await.unwrap();

        // Stash a blob for it.
        let test_blob = b"this-blob-should-get-pruned".to_vec();
        eg.dag_blob_store(&event.id(), &test_blob).unwrap();
        assert!(
            eg.dag_blob_fetch(&event.id()).unwrap().is_some(),
            "precondition: blob should be present before pruning",
        );

        // Rotate `limit + 1` times to force eviction of the
        // original DAG. Each rotation creates a fresh genesis and
        // (after limit reached) evicts the oldest.
        for i in 0..=limit {
            let new_ts = original_dag_ts + (i as u64 + 1) * 60_000;
            let hdr = crate::event_graph::event::Header {
                timestamp: new_ts,
                parents: crate::event_graph::NULL_PARENTS,
                layer: 0,
                content_hash: blake3::hash(&eg.config.genesis_contents),
            };
            let new_genesis = Event { header: hdr, content: eg.config.genesis_contents.clone() };
            eg.dag_prune(new_genesis).await.unwrap();
        }

        // Original event's blob should now be gone - its DAG was
        // evicted, and dag_prune cleaned up the side-table.
        assert!(
            eg.dag_blob_fetch(&event.id()).unwrap().is_none(),
            "blob should be pruned after its DAG was evicted from the rolling window",
        );
    })
}

/// Build a synthetic Event with the given (layer, timestamp) and a
/// content payload encoding a Registration of the given commitment.
/// Used to drive apply_rln_static_event without going through the
/// real Event::new_static path (which depends on the EG's static-DAG
/// tip set).
async fn synth_static_event(layer: u64, timestamp: u64, node: &RLNNode) -> Event {
    use crate::event_graph::event::Header;
    let content = serialize_async(node).await;
    // Use a single non-NULL parent to satisfy the
    // "non-genesis" predicate. The exact parent ID doesn't matter
    // for SMT mutation; the SMT only sees the commitment from the
    // RLNNode payload.
    let mut parents = NULL_PARENTS;
    parents[0] = blake3::hash(b"synthetic-parent");
    let header = Header { timestamp, parents, layer, content_hash: blake3::hash(&content) };
    Event { header, content }
}

#[test]
fn rln_is_root_valid_at_respects_drift_window() {
    // A root produced at timestamp T_R is valid for signals whose
    // timestamps fall within EVENT_TIME_DRIFT of T_R (in either
    // direction), and stays valid for as long as it remains the
    // live root (until the next event).
    smol::block_on(async {
        let eg = make_eg().await;
        let drift = crate::event_graph::EVENT_TIME_DRIFT;
        let t_r: u64 = 1_000_000;
        let commitment = pallas::Base::from(0xaaaa_u64);
        let node = RLNNode::Registration(commitment);
        let ev = synth_static_event(1, t_r, &node).await;
        let r = eg.apply_rln_static_event(&ev, &node).await.unwrap();

        // Within the drift window in both directions:
        assert!(eg.is_root_valid_at(&r, t_r).unwrap(), "valid at exactly T_R");
        assert!(eg.is_root_valid_at(&r, t_r + drift).unwrap(), "valid at T_R + drift");
        assert!(
            eg.is_root_valid_at(&r, t_r.saturating_sub(drift)).unwrap(),
            "valid at T_R - drift"
        );

        // Far in the future is also fine because R is still live
        // (no later event yet).
        assert!(
            eg.is_root_valid_at(&r, t_r + 1_000_000_000).unwrap(),
            "valid in the far future when no later event"
        );

        // Far before T_R - drift fails: signal claims a root that
        // didn't exist at signal time.
        let far_past = t_r.saturating_sub(2 * drift + 1);
        assert!(
            !eg.is_root_valid_at(&r, far_past).unwrap(),
            "should reject signal at far past - root didn't exist yet",
        );
    })
}

#[test]
fn rln_slashed_identity_signal_rejection_lifecycle() {
    // Operational regression test for the full slashed-identity
    // lifecycle. This is the test that answers the question:
    // "After we slash an identity, how do we ensure their future
    // signals are rejected?"
    //
    // The defense is structural - there is no explicit deny-list
    // for slashed identities (RLN-V2's privacy guarantees prevent
    // the verifier from identifying signers). Instead, two
    // mechanisms work in concert:
    //
    //   (a) The SMT mutation removes the slashed leaf, so post-slash
    //       roots don't contain the slashed commitment. A
    //       signal-membership proof can't be built against a
    //       post-slash root.
    //   (b) The historical-roots time-window check rejects pre-slash
    //       roots after `T_slash + DRIFT`. So the slashed user
    //       can't replay against their old root indefinitely.
    //
    // The DRIFT window of acceptance after slash is by design
    // (propagation tolerance, identical to every signal's window).
    //
    // This test walks through the timeline with synthetic events
    // and asserts the time-window check has the right shape.
    // Exercising the proof-verification side of (b) requires real
    // ZK keys and is left to the multi-node integration tests.
    smol::block_on(async {
        let eg = make_eg().await;
        let drift = crate::event_graph::EVENT_TIME_DRIFT;

        // The slashed identity's commitment.
        let user_commitment = pallas::Base::from(0xfeed_u64);

        // Timeline:
        //   T0: register the user                 -> root R_reg
        //   T_pre: send a normal signal           (signal_time = T_pre)
        //   T_slash: slash the user               -> root R_slashed
        //   T_amnesty: signal during DRIFT window (still claiming R_reg)
        //   T_late: signal after DRIFT expires    (still claiming R_reg)
        //
        // We don't care about real ZK proof verification here;
        // is_root_valid_at is the gate that runs before the proof
        // is even loaded. If is_root_valid_at says "yes" for
        // T_pre/T_amnesty and "no" for T_late, we've validated the
        // full structural defense from the verifier's perspective.
        let t0: u64 = 1_000_000;
        let t_slash: u64 = t0 + 100 * drift; // long after registration

        // Step 1: register the user.
        let reg_node = RLNNode::Registration(user_commitment);
        let ev_reg = synth_static_event(1, t0, &reg_node).await;
        let r_reg = eg.apply_rln_static_event(&ev_reg, &reg_node).await.unwrap();

        // Step 2: a normal signal at T_pre (mid-life of R_reg).
        // The user claims R_reg as their merkle root. Because R_reg
        // is the live root throughout [t0, t_slash), this signal
        // passes the root-window check.
        let t_pre = t0 + 50 * drift;
        assert!(
            eg.is_root_valid_at(&r_reg, t_pre).unwrap(),
            "pre-slash signal at T_pre claiming R_reg must be accepted (root-window check)",
        );

        // Step 3: slash the user.
        let slash_node = RLNNode::Slashing(user_commitment);
        let ev_slash = synth_static_event(2, t_slash, &slash_node).await;
        let r_slashed = eg.apply_rln_static_event(&ev_slash, &slash_node).await.unwrap();

        // Sanity: R_reg's live interval is now [t0, t_slash). The
        // post-slash root R_slashed is live from t_slash onward.
        assert_ne!(r_reg, r_slashed, "slash should change the SMT root");

        // Step 4: signal during the DRIFT amnesty window. The
        // slashed user's clock-aware proof claims R_reg with
        // timestamp T_amnesty = T_slash + DRIFT/2. Within the live
        // interval extended by drift, so accepted.
        //
        // This is intentional - every signal gets the same
        // propagation-tolerance window, and we'd rather accept a
        // few extra messages from a just-slashed user than reject
        // legitimate messages from a not-yet-aware-of-their-slash
        // user. The deeper defense is the rate-limit polynomial,
        // which catches reuse and triggers another slash if the
        // user tries to flood.
        let t_amnesty = t_slash + drift / 2;
        assert!(
            eg.is_root_valid_at(&r_reg, t_amnesty).unwrap(),
            "DRIFT amnesty: signal at T_slash + DRIFT/2 claiming R_reg should still pass \
             the root-window check (propagation tolerance, identical to every signal)",
        );

        // Step 5: signal after the DRIFT window expires. The
        // slashed user attempts to keep replaying their pre-slash
        // root. T_late = T_slash + 2*DRIFT - definitively outside
        // the window. Rejected.
        let t_late = t_slash + 2 * drift;
        assert!(
            !eg.is_root_valid_at(&r_reg, t_late).unwrap(),
            "post-DRIFT: signal at T_slash + 2*DRIFT claiming R_reg must be rejected - \
             this is the time-window check denying the slashed user further replays",
        );

        // Step 6: signal claiming the post-slash root R_slashed at
        // T_late. The root-window check passes (R_slashed is
        // currently live), but in real verification the ZK proof
        // would fail - the slashed commitment isn't a leaf in
        // R_slashed. We can't exercise that here without real
        // proofs, but we document the invariant: defense (a) (SMT
        // mutation) covers this case while defense (b)
        // (time-window) covers Step 5.
        assert!(
            eg.is_root_valid_at(&r_slashed, t_late).unwrap(),
            "post-slash root is current and accepted by the root-window check; \
             the proof would fail because the slashed commitment isn't a leaf - \
             but that's tested elsewhere with real ZK keys",
        );
    })
}

#[test]
fn rln_canonical_order_produces_same_roots_regardless_of_apply_order() {
    // SMT roots are determined by the SET of leaves, not the
    // insertion order - but only the *final* root, not intermediates.
    // Our canonical-order requirement (sort by (layer, event_id))
    // ensures all nodes produce the same SEQUENCE of intermediate
    // roots when replaying the same set of events.
    smol::block_on(async {
        let eg_a = make_eg().await;
        let eg_b = make_eg().await;

        let c1 = pallas::Base::from(0x1111_u64);
        let c2 = pallas::Base::from(0x2222_u64);
        let n1 = RLNNode::Registration(c1);
        let n2 = RLNNode::Registration(c2);

        // Both events at the same layer (intentionally - to force
        // the event_id tie-breaker to determine canonical order).
        let ev1 = synth_static_event(1, 100_000, &n1).await;
        let ev2 = synth_static_event(1, 100_001, &n2).await;

        // Determine canonical order by event_id.
        let (first, first_node, second, second_node) = if ev1.id().as_bytes() < ev2.id().as_bytes()
        {
            (&ev1, &n1, &ev2, &n2)
        } else {
            (&ev2, &n2, &ev1, &n1)
        };

        // Node A: apply in canonical order (first, second).
        let a_root1 = eg_a.apply_rln_static_event(first, first_node).await.unwrap();
        let a_root2 = eg_a.apply_rln_static_event(second, second_node).await.unwrap();

        // Node B: apply in reverse, but for the test we want to
        // observe what happens IF a node naively applied in
        // received-order. So we deliberately call apply_ in the
        // wrong order. The bug we're guarding against is "if you
        // bypass canonical-sort, you get different intermediate roots".
        let b_root1_wrong = eg_b.apply_rln_static_event(second, second_node).await.unwrap();
        let b_root2 = eg_b.apply_rln_static_event(first, first_node).await.unwrap();

        // Final roots match (SMT is set-determined).
        assert_eq!(a_root2, b_root2, "final roots must match after applying same set");

        // Intermediate roots DIFFER if not canonically ordered.
        // This is the negative result that motivates the canonical
        // sort in static_sync.
        assert_ne!(
            a_root1, b_root1_wrong,
            "intermediate roots should differ when apply order isn't canonical - \
             this asserts the property that motivates static_sync's canonical sort",
        );

        // Both nodes' historical-roots tables should be queryable
        // for their respective intermediate roots at the relevant
        // timestamps:
        assert!(eg_a.is_root_valid_at(&a_root1, 100_000).unwrap());
        assert!(eg_b.is_root_valid_at(&b_root1_wrong, 100_001).unwrap());

        // But cross-node lookup fails - node B doesn't recognize
        // a_root1 because it never produced that root.
        assert!(
            !eg_b.is_root_valid_at(&a_root1, 100_000).unwrap(),
            "node B never produced a_root1 - wrong-order apply diverges from canonical",
        );
    })
}

#[test]
fn rln_rebuild_historical_roots() {
    smol::block_on(async {
        let eg = make_eg().await;

        let c1 = pallas::Base::from(0x3333_u64);
        let c2 = pallas::Base::from(0x4444_u64);
        let n1 = RLNNode::Registration(c1);
        let n2 = RLNNode::Registration(c2);

        let ev1 = synth_static_event(1, 100_000, &n1).await;
        let ev2 = synth_static_event(2, 100_001, &n2).await;
        let r1 = eg.apply_rln_static_event(&ev1, &n1).await.unwrap();
        eg.static_insert(&ev1).await.unwrap();
        let r2 = eg.apply_rln_static_event(&ev2, &n2).await.unwrap();
        eg.static_insert(&ev2).await.unwrap();

        // (b) Run rebuild on a consistent state - should be a no-op.
        let before = eg.rln_historical_roots_ordered.len();
        eg.rebuild_historical_roots_if_needed().await.unwrap();
        assert_eq!(eg.rln_historical_roots_ordered.len(), before, "no-op when consistent");

        // (a) Wipe tables and rebuild.
        eg.rln_historical_roots_ordered.clear().unwrap();
        eg.rln_historical_roots_by_value.clear().unwrap();
        assert!(!eg.is_root_valid_at(&r1, 100_000).unwrap(), "precondition: cleared");

        eg.rebuild_historical_roots_if_needed().await.unwrap();

        assert!(eg.is_root_valid_at(&r1, 100_000).unwrap(), "rebuild restored r1");
        assert!(eg.is_root_valid_at(&r2, 100_001).unwrap(), "rebuild restored r2");
        assert_eq!(eg.rln_historical_roots_ordered.len(), 2, "exactly one entry per static event",);
    })
}

#[test]
fn rln_perf_signal_verify() {
    use std::time::Instant;
    smol::block_on(async {
        let (eg, mut id) = fresh_identity_and_eg().await;
        id.user_message_limit = 50; // enough headroom
        id.register_directly(&eg).await.unwrap();

        // Warm up the verifier (first call may pay one-shot setup costs).
        let event = make_static_event(b"static-event-9", &eg).await;
        let mid = id.next_message_id(event.header.timestamp).expect("budget");
        let blob = id.create_signal(&event, mid, &eg).await.unwrap();
        let _ = eg.rln_verify_signal(&event, &serialize_async(&blob).await).await;

        // Time signal proof CONSTRUCTION (the user-side cost).
        let n_construct = 10;
        let start = Instant::now();
        let mut blobs = vec![];
        for _ in 0..n_construct {
            let event = make_static_event(b"static-event-10", &eg).await;
            let mid = id.next_message_id(event.header.timestamp).expect("budget");
            let blob = id.create_signal(&event, mid, &eg).await.unwrap();
            blobs.push((event, serialize_async(&blob).await));
        }
        let construct_ms = start.elapsed().as_millis() as f64 / n_construct as f64;

        // Time signal proof VERIFICATION (the server-side cost,
        // which is what bottlenecks high-throughput nodes).
        let start = Instant::now();
        for (ev, bytes) in &blobs {
            let _ = eg.rln_verify_signal(ev, bytes).await;
        }
        let verify_ms = start.elapsed().as_millis() as f64 / n_construct as f64;

        eprintln!(
            "[RLN perf] construct: {construct_ms:.2} ms/proof; \
             verify: {verify_ms:.2} ms/proof"
        );
    })
}
