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
  - `Has { inserted: bool }` — peer has the event (freshly inserted,
    already known, **or found in a retained older rotation slot** — the
    historical check runs before nacking `TooOld`, so a peer still
    holding a rotated-away message answers `Has` instead of `TooOld`)
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
    event id, plaintext privmsg, state, attempts, `superseded_by` link.
  - Receipt aggregation keyed by (event_id, peer channel); a foreign
    event whose DAG ancestry includes our outbound event also counts as
    positive delivery evidence (free secondary signal, no wire change).
  - Delivery monitor — rebroadcast-first when reconnected and synced.
    Silence alone never triggers recreation while the rotation window
    is open; recreate only when the window is closed (locally or via
    `TooOld` nacks) with no holder answering, or when every observed
    reply is an explicit rejection. `NotSynced`/`Busy`/silence mean
    retry, not evidence.
- Recreate = new event from stored plaintext (fresh parents/timestamp,
  fresh saltbox nonce), new RLN slot when RLN is enabled
  (`BudgetExhausted` parks the attempt until the next epoch), bounded
  attempt count, `superseded_by` chain for local correlation.

Non-goals: read receipts stored in the event graph (pollutes the DAG,
burns RLN budget, leaks linkability); IRC-surface receipt display;
`bin/app` integration (delivery-state UI, subscriptions, or any other
app-side changes — darkirc is the reference implementation here and the
app consumes the same generic pipe in a follow-up change); pull-based
possession verification via `EventReq`; `StaticPut` status (nickserv
already has a deferred-broadcast queue); recipient-identifying receipts
(nodes are anonymous; this is delivery-to-network evidence only). A
`Privmsg` dedup field (`uid`) was considered and deliberately deferred:
with the historical-slot check and the silence-never-recreates rule,
recreation almost never overlaps with "someone already rendered the
original", and adding such a field later is an additive version bump
while removing it after shipping would be a wire break. Known accepted
cost: rare duplicate renders during mixed-version rollout windows and
under adversarial fake-nack griefing.

## Capabilities

### New Capabilities

- `msg-delivery-status`: outcome reporting for `EventPut` broadcast,
  delivery-state exposure to applications, and the sender-side
  rebroadcast/recreate policy implemented by darkirc as the reference
  consumer.

### Modified Capabilities

(none — no existing specs change.)

## Impact

- `src/event_graph/proto.rs`, `src/event_graph/mod.rs` — new wire message,
  status emission, receipt publisher. Protocol-handler surface: must stay
  panic-free on untrusted input, keep flood/strike policing unchanged.
- `bin/darkirc` (client.rs, server.rs, lib.rs, crypto/rln.rs interplay) —
  outbound table, monitor task, recreate path.
- `bin/tau/taud` and `bin/app` — untouched consumers; they may adopt the
  same `receipt_pub` pipe in follow-up changes.
- No `Privmsg` or other consensus/content serialization changes in this
  change.
- Wire compatibility: peers without `EventPutStatus` support simply never
  reply; senders treat silence as "retry later, never a verdict", so
  correctness does not depend on reply availability.
- Per repo policy, `event_graph` protocol changes require
  `@anon-security-review` before apply/archive.
