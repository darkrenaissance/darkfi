# Proposal: darkirc-delivery-status

## Why

When a darkirc node is synced but offline (or loses all peers), outgoing
messages are committed to the local DAG and broadcast into the void. Nothing
ever retries them, and once the DAG rotation window passes, every peer
silently rejects them (`event.header.timestamp < genesis_ts`) — the message
is permanently undeliverable and the user never learns it was lost. There is
also no delivery feedback of any kind today: `EventPut` is fire-and-forget,
so clients such as `app` cannot display whether a message reached the
network.

## What Changes

- New p2p message `EventPutStatus` in `src/event_graph/proto.rs`: a reply
  sent by the receiver of an `EventPut` describing the outcome. Enum
  variants use explicit `u8` discriminants for a stable wire format:
  - `Has { inserted: bool }` — peer has the event (freshly inserted, or
    already known from earlier propagation)
  - `Nack { reason }` with coarse reasons: `TooOld`, `NotSynced`,
    `Invalid`, `Busy`. No fine-grained validation detail (avoids turning
    nacks into a validation oracle).
- `ProtocolEventGraph::handle_event_put` emits statuses at each outcome
  (inserted, already-known, too-old, not-synced, invalid/failed).
- `EventGraph` gains a `receipt_pub` publisher (same pattern as
  `event_pub`) that republishes every inbound `EventPutStatus` unfiltered.
  The event layer is a dumb pipe; delivery policy lives in applications.
- darkirc (`bin/darkirc`):
  - Persistent outbound table (kvdb tree, written before broadcast):
    event id, `uid`, plaintext privmsg, state, attempts.
  - Receipt aggregation keyed by (event_id, peer channel).
  - Delivery monitor: rebroadcast-first when reconnected and synced;
    recreate when nacked `TooOld`, when all responses are nacks, or when
    zero responses persist after bounded retries.
  - Recreate = new event from stored plaintext (fresh parents/timestamp,
    fresh saltbox nonce, same `uid`), new RLN slot when RLN is enabled
    (`BudgetExhausted` parks the attempt until the next epoch), bounded
    attempt count.
- `Privmsg` gains a client-generated `uid` field (constant across
  recreates) with a version bump, so receiving clients dedup a recreated
  message against its original. Serialization change must be sequenced
  with the in-flight `darkirc-mod` content-tag work.
- `app` (`bin/app`): subscribes to `receipt_pub` in-process and maps
  `uid` to per-message delivery state (sending / delivered / failed);
  message identity switches to `uid`-based dedup.

Non-goals: read receipts stored in the event graph (pollutes the DAG,
burns RLN budget, leaks linkability); IRC-surface receipt display (only
the app displays them); pull-based possession verification via
`EventReq`; `StaticPut` status (nickserv already has a deferred-broadcast
queue); recipient-identifying receipts (nodes are anonymous; this is
delivery-to-network evidence only).

## Capabilities

### New Capabilities

- `msg-delivery-status`: outcome reporting for `EventPut` broadcast,
  delivery-state exposure to applications, sender-side rebroadcast/recreate
  policy for darkirc, and client-side dedup/display of delivery state in
  the app.

### Modified Capabilities

(none — `chatview` is untouched; app-side rendering is covered by
`msg-delivery-status` requirements.)

## Impact

- `src/event_graph/proto.rs`, `src/event_graph/mod.rs` — new wire message,
  status emission, receipt publisher. Protocol-handler surface: must stay
  panic-free on untrusted input, keep flood/strike policing unchanged.
- `bin/darkirc` (client.rs, server.rs, lib.rs, crypto/rln.rs interplay) —
  outbound table, monitor task, recreate path.
- `bin/app/src/plugin/darkirc.rs` — send path wrapping, `uid` dedup,
  receipt subscription.
- `bin/tau/taud` — unaffected consumer; may adopt the same pipe later.
- Wire compatibility: peers without `EventPutStatus` support simply never
  reply; senders treat silence as "no response" (drives rebroadcast
  retries, never correctness). New `Privmsg` field requires version-aware
  decoding.
- Sequencing: coordinate `Privmsg` serialization with `darkirc-mod`.
- Per repo policy, `event_graph` protocol changes require
  `@anon-security-review` before apply/archive.
