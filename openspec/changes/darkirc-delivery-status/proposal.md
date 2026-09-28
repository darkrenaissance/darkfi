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
  - Durable global FIFO of logical messages, preserving plaintext and
    enqueue order across restart. Only the oldest unacknowledged message
    creates/exposes an event when synced and sufficiently connected;
    later messages cannot bypass it. Every generation is retained before
    network exposure, including its unchanged event/blob independently
    of DAG pruning and old-to-new `superseded_by` links.
  - ACK = `Has` for any head generation, or an accepted foreign event
    whose ancestry includes that generation. ACK durably dequeues the
    logical message exactly once and permits the next send. Receipt
    aggregation uses ephemeral peer-channel identity, not peer addresses.
  - Head-only worker retries unchanged with capped backoff until ACK,
    without a lifetime or attempt cap. Recreate only after expiry
    (local rotation knowledge or `TooOld`) and a rebroadcast round
    without ACK. Silence, `Invalid`, `NotSynced`, and `Busy` alone
    neither recreate nor dequeue. Local expiry permits recreation even
    when no replies have ever arrived.
  - Bound storage, including retained generation metadata. Reject new
    enqueues explicitly when full; park the head if recreation cannot
    fit. Never evict unacknowledged messages to make progress.
- Recreate = new event from stored plaintext (fresh parents/timestamp,
  fresh saltbox nonce), new RLN slot when RLN is enabled, same queue
  position, and `superseded_by` chain for local correlation. Exhausted
  budget, missing identity, and processing/storage failures park the
  head rather than discard it or send an unproven replacement.

Non-goals: read receipts stored in the event graph (pollutes the DAG,
burns RLN budget, leaks linkability); IRC-surface receipt/status display
(enqueue rejection and blocked-send error notices to connected clients
remain in scope);
`bin/app` integration (delivery-state UI, subscriptions, or any other
app-side changes — darkirc is the reference implementation here and the
app consumes the same generic pipe in a follow-up change); pull-based
possession verification via `EventReq`; `StaticPut` status (nickserv
already has a deferred-broadcast queue); recipient-identifying receipts
(nodes are anonymous; this is delivery-to-network evidence only);
recipient display ordering (FIFO governs local submission, not network
propagation); automatic fail-and-skip or per-conversation queues. A
`Privmsg` dedup field (`uid`) was considered and deliberately deferred:
the historical-slot check and the no-recreation-on-silence rule while
the window is open reduce, but do not eliminate, overlap with "someone
already rendered the original". Adding such a field later is an
additive version bump while removing it after shipping would be a wire
break. Accepted cost: duplicate renders during mixed-version rollout,
with unreachable holders or lost replies, and under adversarial
fake-nack griefing; their frequency is not established. Statuses are
unverified evidence: even one peer with knowledge of an event id can
lie `Has` and suppress recovery without forwarding the event.
The global FIFO intentionally accepts indefinite head-of-line blocking,
including across conversations. Backoff and storage bounds constrain
resource use, not total lifetime retries or successful delivery.

## Capabilities

### New Capabilities

- `msg-delivery-status`: outcome reporting for `EventPut` broadcast,
  delivery-state exposure to applications, and a durable ACK-driven FIFO
  with head-only rebroadcast/recreation implemented by darkirc as the
  reference consumer.

### Modified Capabilities

(none — no existing specs change.)

## Impact

- `src/event_graph/proto.rs`, `src/event_graph/mod.rs` — new wire message,
  status emission, receipt publisher. Protocol-handler surface: must stay
  panic-free on untrusted input, keep flood/strike policing unchanged.
- `bin/darkirc` (client.rs, server.rs, lib.rs, crypto/rln.rs interplay) —
  outbound FIFO, head-only worker, enqueue/error path, recreate path.
- `bin/tau/taud` and `bin/app` — untouched consumers; they may adopt the
  same `receipt_pub` pipe in follow-up changes.
- No `Privmsg` or other consensus/content serialization changes in this
  change.
- Wire compatibility is conditional on the first task proving unknown
  message tolerance. If old channels reject the new message type, stop
  and revisit capability gating before implementation continues. Old
  peers do not reply; silence alone is not a verdict, but local expiry
  can justify recreation without replies and may cause duplicates.
- Persisted plaintext is sensitive local data, including after rollback.
- Stop for human review before implementation changes touching RLN,
  crypto, circuits, or canonical serialization. This plan does not
  authorize changes to those protected areas.
- Per repo policy, `event_graph` protocol changes require
  `@anon-security-review` before marking ready to apply/archive; this
  triage does not replace CI or human review.
