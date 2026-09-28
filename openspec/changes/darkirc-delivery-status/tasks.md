# Tasks: darkirc-delivery-status

## 1. Wire compatibility spike (blocks everything)

- [ ] 1.1 Verify unknown-message tolerance: using the event-graph test
  harness (`src/event_graph/test_helpers.rs`), connect two nodes where
  only one registers an `EventPutStatus` dispatch, send one from the
  other, and confirm the receiving channel stays up (no stop/strike/
  panic). Record the outcome in the change notes; if unknown ids
  destabilize channels, stop and re-discuss gating before proceeding.
  Verify via a new test in `src/event_graph/tests.rs`.

## 2. Event-graph layer (`src/event_graph`)

- [ ] 2.1 Define `EventPutStatus`/`EventPutResult`/`NackReason` in
  `proto.rs` with explicit `u8` discriminants, `impl_p2p_message!` with
  default metering, and register dispatch/subscription in
  `ProtocolEventGraph::init`. Verify `make` builds and clippy is clean.
- [ ] 2.2 Emit statuses from `handle_event_put` at every outcome per the
  design table (NotSynced, Has{false} duplicate, historical-slot check
  before TooOld, Invalid, Busy, Has{true} inserted), unicast on the
  receiving channel; strike/flood paths unchanged and silent. Verify
  each emission point with a two-node test asserting the exact reply
  variant, including the retained-slot `Has{false}` case across a
  rotation.
- [ ] 2.3 Add `receipt_pub: Publisher<(EventPutStatus, ChannelPtr)>` and
  `receipt_subscribe()` to `EventGraph`; republish every inbound status
  unfiltered from the per-channel handler. Verify with a test that
  receives both own-origin and relayed statuses on the subscription.
- [ ] 2.4 Decode hardening: unknown `EventPutResult`/`NackReason`
  variant or malformed body is dropped with a warning, no panic, no
  strike, channel stays connected. Verify with a fuzz-style unit test
  feeding truncated/random payloads through the decoder.

## 3. darkirc outbound tracking (`bin/darkirc`)

- [ ] 3.1 Add the `darkirc_outbound` kvdb tree and serializable record
  (event id, plaintext Privmsg fields, state, attempts, ts,
  `superseded_by`), plus helpers to insert/close/update records. Verify
  with round-trip serialization + tree unit tests in `server.rs`.
- [ ] 3.2 Write the outbound record in `publish_events` before
  `p2p.broadcast`; close as Delivered on any `Has`. Verify with a unit
  test that a crash-restart (reopen kvdb) still finds the record
  Pending.

## 4. darkirc delivery monitor (`bin/darkirc`)

- [ ] 4.1 Receipt aggregation task: subscribe `receipt_pub`, filter to
  tracked ids, dedupe per (event id, channel address), update records;
  treat an ancestry reference (foreign event whose parents' closure
  contains a tracked id) as positive delivery evidence. Verify with
  unit tests driving synthetic statuses and a synthetic child event
  through the aggregator.
- [ ] 4.2 Sweep + rebroadcast: periodic sweep gated by
  `is_synced() && connection_count >= K`, rebroadcasting the original
  `EventPut` (event + blob from local DAG) with backoff up to `R_MAX`
  rounds, then continuing at the slow rate. Verify with a multi-node
  harness test that a peer which already has the event answers
  `Has{inserted:false}` and the record closes without recreation, and
  that pure silence (status-incapable peer) never produces a
  replacement while the window is open.
- [ ] 4.3 Recreate triggers per design: window-closed-with-no-holder
  (local rotation knowledge or TooOld nacks) and explicit all-nack with
  at least one `Invalid`; `NotSynced`/`Busy`/silence mean retry. Build
  a fresh event from stored plaintext (fresh nonce, fresh
  parents/timestamp), park on RLN `BudgetExhausted`, cap attempts at
  `A_MAX`, surface failure to the client on cap. Verify with harness
  tests: rotation-forced recreate writes a superseding record linked
  via `superseded_by`; budget exhaustion parks; attempt cap reaches
  Failed; `NotSynced`-only replies do not recreate.
- [ ] 4.4 End-to-end darkirc scenario test: node A publishes while
  disconnected from B, A reconnects after DAG rotation, B must end up
  holding a recreate-generation event and A's record must show the
  `superseded_by` link; a second scenario where B already holds the
  original in a retained slot must close A's record as Delivered with
  no recreate. Verify both assertions in the multi-node harness.

## 5. Hardening and review gate

- [ ] 5.1 Full workspace gates green: `make`, `make clippy`, `make test`
  (proofs + contracts built first per AGENTS.md); confirm no
  `unwrap`/`expect`/`panic!` on any new attacker-controlled decode path
  and no peer addresses logged with receipt state.
- [ ] 5.2 Invoke `@anon-security-review` on the full diff; treat FAIL as
  blocking and address findings before marking the change ready to
  apply. Verify the review verdict is recorded in the change notes.
