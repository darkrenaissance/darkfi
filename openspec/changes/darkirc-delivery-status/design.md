# Design: darkirc-delivery-status

## Context

Today `publish_events` (bin/darkirc/src/irc/client.rs) commits an event
locally, calls `p2p.broadcast(&EventPut(...))`, and never learns anything
about the outcome. The receiving handler
(`ProtocolEventGraph::handle_event_put`, src/event_graph/proto.rs)
silently `continue`s on every skip path (duplicate, too old, unsynced,
invalid). `synced` is a one-way latch, so the existing `args_queue`
covers only the initial-sync window, not later disconnections; once
`dag_prune_task` rotates the DAG, peers reject any event with
`timestamp < genesis_ts` forever. See proposal.md for motivation.

Constraints that shape the design:

- `ProtocolEventGraph` is per-peer-connection; the only shared state is
  on `EventGraph` (pattern: `event_pub`/`static_pub` publishers).
- darkirc and `bin/app` both publish chat events directly
  (`client.rs::publish_events` and `app/src/plugin/darkirc.rs::handle_send`);
  taud uses the same protocol without chat semantics.
- RLN is currently inactive on the network (empty blobs), but the
  enabled path must stay correct: recreations must reserve a fresh
  message slot or risk self-slashing.
- This plan is not crypto sign-off. Stop for human review before any
  implementation change touching RLN, crypto, circuits, or canonical
  serialization; the security-agent verdict does not replace that review.

## Goals / Non-Goals

**Goals:**

- Outcome replies for `EventPut` with zero policy in the generic layer.
- darkirc: a durable global FIFO, oldest logical message first, retained
  until acknowledged; rebroadcast unchanged and recreate only after
  expiry. Preserve order across restart and use the existing RLN-safe
  send path when rate limiting is enabled.
- Stable wire format (`u8` discriminants), no new panics on untrusted
  input.

**Non-Goals:**

- Pull-based possession verification (`EventReq` challenges) — dropped;
  status replies plus ancestry references are the only signals, treated
  as unverified hints, not proof of delivery to a recipient.
- `bin/app` integration — out of scope entirely (no UI, subscriptions,
  or send-path changes there). darkirc is the reference implementation;
  the app consumes the same generic `receipt_pub` pipe in a follow-up
  change that copies this logic.
- `Privmsg` payload changes (`uid`/dedup field) — deferred (see
  Alternatives Considered). No content serialization changes in this
  change.
- Status for `StaticPut`, IRC-surface receipt/status display,
  read-by-recipient semantics, changes to strike/flood policing or the
  relay path. Enqueue rejection and blocked-send error notices to
  connected clients remain in scope; a delivery-status UI does not.
- Recipient display ordering and wire-level deduplication. FIFO governs
  local submission only; propagation and delayed generations can still
  arrive out of order or render twice.

## Decisions

### D1: Wire message — `EventPutStatus`

```rust
#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct EventPutStatus {
    pub event_id: blake3::Hash,
    pub result: EventPutResult,
}

#[repr(u8)]
enum EventPutResult {
    Has { inserted: bool } = 0,
    Nack { reason: NackReason } = 1,
}

#[repr(u8)]
enum NackReason { TooOld = 0, NotSynced = 1, Invalid = 2, Busy = 3 }
```

- Name: `EventPutStatus` (not `EventPutRep`) to avoid confusion with
  `EventRep`, which answers `EventReq`.
- Discriminants are explicit `u8`s; encoding stays the
  `darkfi-serial` derive (variant tag `u8` + payload, verified in
  `derive-internal`). Payload is 34 bytes; ~70 bytes on the wire
  including the per-message frame (magic + command + VarInt length) —
  ~14% of an RLN-less `EventPut`, ~2-3% of an RLN-carrying one, per
  direct edge.
- Unknown variant tags must decode-fail into a warn-and-drop, not a
  panic and not a strike.
- Payload carries only `event_id` + outcome — never channel, nick, or
  content (privacy: the reply leaks nothing beyond what the event itself
  already revealed to that peer).
- Unicast reply on the channel the `EventPut` arrived from; never
  relayed. Uses the default metering configuration like the other
  event-graph messages.

Alternative considered: separate ack/nack messages — rejected, one
subscription and one dispatch is simpler and the enum keeps them
versioned together.

### D2: Emission points in `handle_event_put`

| Path (in current code order) | Reply |
|---|---|
| `!is_synced()` skip | `Nack { NotSynced }` |
| `main_tree` duplicate | `Has { inserted: false }` |
| `timestamp < genesis_ts` | check retained slots first: present → `Has { inserted: false }`; absent → `Nack { TooOld }` |
| established structural / proof / parent invalidity | `Nack { Invalid }` |
| transient parent-fetch failure or internal processing failure | `Nack { Busy }` |
| internal insert error after verification | `Nack { Busy }` |
| `insert_verified_signal` success | `Has { inserted: true }` |

The historical-slot check is load-bearing: the current duplicate check
only consults the current slot's `main_tree`, so without it a peer
holding a rotated-away message nacks `TooOld` exactly like a peer that
never saw it — collapsing "delivered but idle" into "never delivered"
and forcing needless recreations. Checking `dag_store`'s retained slots
(`max_dags` window) before nacking separates the two.

`Has{inserted:false}` (live and historical) is the dominant positive
outcome after reconnect-and-rebroadcast, and the only thing
distinguishing "already propagated" from "never arrived". Strike/flood
paths keep current behavior and send nothing. The duplicate check sits
before the flood-window tick, so replying there is free.

### D3: Dumb pipe — `receipt_pub` on `EventGraph`, policy in apps

`EventGraph` gains `receipt_pub: Publisher<(EventPutStatus, ChannelPtr)>`
plus a `receipt_subscribe()` accessor, republishing every inbound status
unfiltered. No registry, no retry state at this layer.

Alternative: a generic outbound-registry with retry policy inside
`EventGraph` — rejected: retry policy is app-specific (darkirc recreates
chat, taud would want its own rules, the app embeds evgr in-process and
needs the events for UI, not policy). Keeping the generic layer stateless
also keeps it out of the security-sensitive blast radius.

### D4: Durable global outbound FIFO

Use the `darkirc_outbound` kvdb tree for ordered logical-message records,
not an unordered set of independently retried event ids. Persist a
stable queue position, plaintext Privmsg fields, enqueue time, optional
active event/blob, supersession metadata, and retry/parked state.
Concurrent enqueues must obtain one durable order; restart preserves it.
Accepting a send means its queue record is durable, including while
offline or initially unsynced. On enqueue/storage-limit failure, report
rejection to the connected client instead of claiming the send is queued.

All initial sends go through this queue. Only the head may materialize
an event or expose it to the network, including via local DAG insertion
and relay/sync. Later messages remain plaintext queue entries until every
predecessor is acknowledged. The sender's queue order is not a guarantee
of recipient display order.

Keep the head's exact active event and blob independently of DAG
retention for unchanged retries after pruning. Keep generation ids and
old-to-new `superseded_by` links until the logical message is acknowledged.
A replacement keeps the same queue position; it is not appended as a
new logical message. Persist its event/blob, active-generation pointer,
and supersession link as one crash-recoverable transition before network
exposure. Recovery must never create two active generations or bypass
an unacknowledged head.

Bound queue storage, including active event/blob and generation metadata.
At capacity, reject new enqueues explicitly; if a replacement cannot be
retained, park the head rather than evicting unacknowledged state.
Acknowledged entries are removable. There is no lifetime or recreation
attempt cap: storage pressure can block progress, not justify dropping
the head. Plaintext-at-rest adds sensitive message history within the
local kvdb trust boundary. Never log or publish that stored plaintext;
rollback does not erase it.

### D5: ACK-driven head-only worker

One task on `IrcServer` owns queue advancement and retry scheduling:

- Subscribe to `receipt_pub`; filter to tracked generation ids and use
  ephemeral connection identity, never peer address, for any per-channel
  aggregation. Do not persist connection identity or log peer addresses.
- An ACK is either `Has` (either inserted value) for any generation of
  the head, or an accepted foreign event whose ancestry includes such
  a generation. Ancestry is local computation, not an additional query.
  Locally generated events alone are not foreign delivery evidence.
- ACK processing durably dequeues the logical message exactly once,
  cancels pending retries/recreation, and permits the next head to run.
  Duplicate/late statuses cannot dequeue another message. Serialize
  ACK, materialization, and supersession transitions so stale work
  cannot resurrect an acknowledged head. Already-broadcast generations
  cannot be recalled.
- A periodic wakeup (30 s) services only the head, with sends gated by
  `is_synced() && connection_count >= K` (K = 2, both session directions
  counted via the existing session APIs). ACK processing is not gated
  on connectivity or the retry timer.
- For a materialized head without ACK, rebroadcast its exact active
  `EventPut`. Backoff doubles from 30 s for `R_MAX` (= 5) rounds, then
  continues at the capped slow rate until ACK. There is no total-round
  limit. Negative replies must not bypass the retry schedule.
- Recreate only after the active event's rotation window has closed
  (local rotation knowledge or `TooOld`) and a rebroadcast round has
  completed without ACK for any generation. This includes locally
  known expiry with no replies ever received. A retained-slot `Has`
  cancels recreation. Silence, `Invalid`, `NotSynced`, and `Busy` alone
  never trigger recreation or advance the queue.

The global FIFO intentionally accepts head-of-line blocking: an
unacknowledged message blocks all later sends, even to other conversations.
There is no automatic fail-and-skip policy.

### D6: Head materialization, recreation, and parking

Reuse the existing send-path internals only for an eligible head:
stored plaintext → `try_encrypt` (fresh nonce for each new generation)
→ `Event::new` (fresh tips/timestamp) → existing RLN reservation/proof
path when enabled. Durably persist the prepared event/blob and any
supersession transition before local insertion or broadcast makes it
network-visible. Then use the existing insertion/broadcast path. Recover
a committed generation by retrying that generation, not by regenerating
it. Unchanged rebroadcast never re-encrypts or reserves a new RLN slot.

Recreation stays at the head and uses a fresh nonce and, when enabled,
a fresh RLN message slot. `BudgetExhausted` parks until epoch rollover;
`MissingIdentity` parks until an identity is available. Processing and
storage failures likewise preserve the head and prevent later sends
from bypassing it. Enqueue/blocked-send error notices may inform a
connected client but do not imply dequeue or terminal failure. Retry
these conditions with bounded rate; never send an unproven replacement
or turn an error into a queue-advancement signal.

The enabled RLN reservation/recovery path requires human review before
implementation. This design does not authorize slot reuse, crypto
changes, or altered RLN semantics.

## Alternatives Considered

**Reference-based confirmation only (no wire message).** Raised in
review: a foreign event whose ancestry includes your event proves the
author's DAG held it, and children arrive for free via sync. Adopted as
the secondary signal in D5. Rejected as the *primary* mechanism because:
(i) silence is not an answer — the last message in a conversation is
never referenced, permanently gray; (ii) the rotation cliff — no event
edge crosses rotations (`timestamp_fits_slot` + parents must exist in
the current slot's tree), so references die exactly at the decision
deadline; (iii) absence is unprovable — `fetch_headers_with_tips` only
extends the requester's frontier, so "peer holds it sterile" and "peer
never got it" are observationally identical, and resolving them needs
`TipReq` frontier probing (truncated at 1024 tips) which is pull-based
verification with extra steps; (iv) no rejection reasons.

**`Privmsg.uid` dedup field.** Deferred. It protects against duplicate
renders when a recreate fires while someone already holds the original.
With D2's historical check and D5's explicit-evidence rule, that overlap
is reduced but not eliminated: mixed-version rollout windows (old peers
can't answer `Has`), unreachable holders, lost replies, adversarial
fake-nack griefing, and recreate-of-recreate can produce duplicates.
Their frequency is not established. Asymmetry decides: adding a payload field later is an
additive `Privmsg.version` bump; removing one after shipping is a wire
break. Revisit on field evidence of annoying duplicates.

**Independent retries and hold-at-send.** Independent record sweeps were
rejected because later messages could overtake an unacknowledged send.
Holding messages before event creation is now part of the FIFO design:
only an eligible head is materialized. Per-conversation queues and
automatic fail-and-skip are not part of this change.

## Risks / Trade-offs

- [Fake statuses: a peer can lie `Has` (suppresses recreate → silent
  loss) or lie `TooOld` (forces recreates → budget burn + duplicates)]
  → only tracked ids affect state; backoff, storage bounds, and enabled
  RLN budget bound resource use/rate, not total lifetime attempts. Even
  one malicious connected peer that knows an id can falsely acknowledge
  it without forwarding. ACK means observed network-delivery evidence,
  not verified delivery to a recipient. This limitation is accepted.
- [Head-of-line blocking and indefinite retention] → intentional
  ACK-only dequeue; no liveness guarantee without ACK. Cap storage and
  reject new enqueues explicitly rather than silently evicting messages.
  Storage exhaustion may also park recreation; no later send bypasses it.
- [Reply amplification: one reply per relay edge] → ~70 bytes on the
  wire against events measured in hundreds of bytes to kilobytes, on
  RLN-rate-limited volume; the flood window is untouched (replies are
  not EventPuts).
- [Unknown-message compatibility: peers without `EventPutStatus`
  receive an unsolicited message type] → verify the channel's behavior
  on unknown message ids during implementation (test with a mixed
  version pair); if unknown ids can destabilize old channels, gate
  replies until a capability flag exists. First implementation task
  resolves this.
- [Duplicate renders without uid: mixed-version transition, adversarial
  nacks, reply loss] → accepted possibility, frequency unknown; documented in the
  proposal's non-goals; additive fix available later if needed.
- [Clock skew: disagreement about rotation boundaries can earn spurious
  `TooOld` nacks] → recreation can duplicate a message and consume
  budget; bounded in rate and storage, not in total attempts or harm.
- [RLN interplay: recreation with a reused slot risks self-slashing] →
  use `reserve_rln_message_id`, park on exhaustion, never reuse a
  reserved slot; require human review and enabled-path tests, including
  restart handling, before relying on this guarantee.
- [Plaintext messages persisted in kvdb] → same trust boundary as the
  existing wallet/RLN secrets; tree is local-only.

## Migration Plan

Additive wire message first (`src/event_graph`), verified against a
mixed-version two-node test; then darkirc queue+worker. No content
serialization changes, so no coordination with `darkirc-mod` is
required. Rollback: the message and records are inert for old code;
reverting leaves a `darkirc_outbound` tree containing sensitive
plaintext and pending state, not a harmless cache. Handle retained data
under the same local storage protections. App integration is
a follow-up change that reuses the same `receipt_pub` pipe and copies
the darkirc queue policy.

## Open Questions

- Exact values of K, `R_MAX`, wakeup interval, and queue storage limit —
  tuning parameters; they must not change head-only service, ACK-only
  dequeue, bounded resource use, or the absence of a lifetime retry cap.
- Whether taud later reuses the same policy for task events — deferred,
  out of scope.
