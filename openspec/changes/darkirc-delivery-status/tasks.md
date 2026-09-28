# Tasks: darkirc-delivery-status

## 1. Compatibility and protected-area review prerequisites

- [ ] 1.1 Verify unknown-message tolerance: using the event-graph test
  harness (`src/event_graph/test_helpers.rs`), connect two nodes where
  only one registers an `EventPutStatus` dispatch, send a status from
  that node to the unregistered receiver, and confirm the receiving
  channel stays up (no stop/strike/
  panic). Record the outcome in the change notes; if unknown ids
  destabilize channels, stop and re-discuss gating before proceeding.
  Verify via a new test in `src/event_graph/tests.rs`.
- [ ] 1.2 Before implementation changes touching RLN, crypto, circuits,
  or canonical serialization, stop for human review of the affected
  path, especially reservation/recovery for head materialization and
  recreation. Record the review outcome and constraints. Do not treat
  planning approval or an agent verdict as crypto sign-off. Flag any
  required dependency, build-script, or proc-macro change for human
  review rather than silently expanding scope.

## 2. Event-graph layer (`src/event_graph`)

- [ ] 2.1 Define `EventPutStatus`/`EventPutResult`/`NackReason` in
  `proto.rs` with explicit `u8` discriminants, `impl_p2p_message!` with
  default metering, and register dispatch/subscription in
  `ProtocolEventGraph::init`. Verify wire tags with round-trip and
  explicit-byte tests without modifying canonical serialization code;
  run `make` and `make clippy`.
- [ ] 2.2 Emit statuses from `handle_event_put` at every outcome per the
  design table (NotSynced, Has{false} duplicate, historical-slot check
  before TooOld, Invalid, Busy, Has{true} inserted), unicast on the
  receiving channel; strike/flood paths unchanged and silent. Verify
  each emission point with a two-node test asserting the exact reply
  variant, including the retained-slot `Has{false}` case across a
  rotation. Distinguish established invalidity (`Invalid`) from
  transient parent-fetch/processing failure (`Busy`) in tests.
- [ ] 2.3 Add `receipt_pub: Publisher<(EventPutStatus, ChannelPtr)>` and
  `receipt_subscribe()` to `EventGraph`; republish every inbound status
  unfiltered from the per-channel handler. Verify with a test that
  receives both own-origin and relayed statuses on the subscription.
- [ ] 2.4 Decode hardening: unknown `EventPutResult`/`NackReason`
  variant or malformed body is dropped with a warning, no panic, no
  strike, channel stays connected. Verify with a fuzz-style unit test
  feeding truncated/random payloads through the decoder and a channel
  test confirming the connection remains usable after malformed input.

## 3. Durable darkirc FIFO (`bin/darkirc`)

- [ ] 3.1 Add the `darkirc_outbound` ordered logical-message records and
  durable enqueue/head/dequeue operations, with plaintext rebuild
  fields, stable queue position, optional active event/blob, generation
  links, and retry/parked state. Verify serialization, concurrent
  enqueue ordering, and reopen/restart preservation with local tests.
- [ ] 3.2 Route every accepted outgoing chat message through durable
  enqueue, including offline and initial-sync sends. Report enqueue
  failure explicitly; do not create events for later entries. Verify
  that accepted messages survive restart, failed enqueues do not report
  success, and neither direct broadcast nor DAG relay/sync exposes a
  later queued message.
- [ ] 3.3 Persist the head's exact event/blob independently of DAG
  retention, and commit replacements with the active-generation pointer
  and old-to-new `superseded_by` link before network exposure. Verify
  fault-injected persistence failures, restart before/after commit,
  unchanged rebroadcast after all local slots prune the event, and one
  active generation at the original queue position.
- [ ] 3.4 Enforce a storage bound covering queued plaintext, active
  event/blob data, and generation metadata. Explicitly reject new
  enqueues at capacity; park recreation if it cannot fit. Verify no
  unacknowledged entry or required correlation data is evicted, and
  ACK'd entries can be removed to reclaim queue space.

## 4. ACK-driven head-only worker (`bin/darkirc`)

- [ ] 4.1 Subscribe to receipts and accepted foreign events; filter to
  tracked head generations and use ephemeral connection identity, not
  channel addresses, for any aggregation. Verify ACK from both `Has`
  variants and foreign ancestry, and no ACK from locally generated
  events or unrelated ids. Do not persist or log peer addresses.
- [ ] 4.2 Serialize ACK/dequeue with head materialization and recreation.
  Verify exactly-once durable advancement, ACK processing while sends
  are gated, duplicate/late replies after restart, and an old-generation
  ACK racing with replacement work without resurrecting the entry or
  removing the next head.
- [ ] 4.3 Service only the head when synced and connection count reaches
  K. Materialize its initial event only then; otherwise retry its exact
  active event with doubling backoff up to `R_MAX`, then a capped slow
  rate indefinitely. Verify with controlled-time tests that negative
  replies cannot bypass backoff, no retry/lifetime limit discards the
  head, and later entries never create or expose events before ACK.
- [ ] 4.4 Recreate only after local expiry or `TooOld` and a completed
  rebroadcast round without ACK. Keep queue position, use fresh
  parents/timestamp/nonce and the reviewed existing RLN path, and retain
  old-generation correlation. Verify silent expiry, retained-slot `Has`
  cancellation, multiple rotations without an attempt cap, and no
  recreation from silence/`Invalid`/`NotSynced`/`Busy` alone.
- [ ] 4.5 Park the head on budget exhaustion, missing identity, and
  processing/storage errors; retry at bounded rate without skipping it.
  Verify enabled-RLN budget rollover and identity recovery under the
  human-reviewed path, unchanged retries consuming no new slot, and no
  unproven enabled-RLN replacement. Verify blocked-send notices do not
  imply dequeue or terminal failure, and later messages remain queued.

## 5. End-to-end FIFO scenarios

- [ ] 5.1 Test offline enqueue of A then B, restart, and reconnect in a
  multi-node harness with the configured connectivity threshold met.
  Assert only A materializes/sends, B stays unexposed until A's ACK,
  and B subsequently becomes the head. Repeat with A parked to confirm
  cross-conversation head-of-line blocking rather than implicit skip.
- [ ] 5.2 Test an already-materialized, unacknowledged head spanning
  disconnection and rotation: first rebroadcast the original, then
  recreate without advancing the queue if no holder ACKs. In a separate
  scenario a peer retains the original and replies `Has{inserted:false}`:
  assert dequeue without recreation. Include a late ACK for an older
  generation after replacement and verify the next message advances
  only once. Do not assume recipient display order or deduplication.

## 6. Hardening and review gates

- [ ] 6.1 Full workspace gates green: `make`, `make clippy`, `make test`
  (proofs + contracts built first per AGENTS.md); confirm no
  `unwrap`/`expect`/`panic!` on any new attacker-controlled decode path
  and no secrets or peer addresses logged with queue/receipt state.
- [ ] 6.2 Invoke `@anon-security-review` on the full implementation diff;
  treat FAIL as
  blocking and address findings before marking the change ready to
  apply/archive. Verify the review verdict is recorded in the change
  notes; planning-only review does not satisfy this implementation gate.
