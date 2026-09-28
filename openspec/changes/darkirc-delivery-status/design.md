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

## Goals / Non-Goals

**Goals:**

- Outcome replies for `EventPut` with zero policy in the generic layer.
- darkirc: durable outbound tracking, rebroadcast-first, bounded
  recreate only on explicit evidence; survives restart; safe with RLN
  on or off.
- Stable wire format (`u8` discriminants), no new panics on untrusted
  input.

**Non-Goals:**

- Pull-based possession verification (`EventReq` challenges) — dropped;
  status replies plus ancestry references are the only signals, treated
  as hints whose worst-case lie is bounded by attempt caps.
- `bin/app` integration — out of scope entirely (no UI, subscriptions,
  or send-path changes there). darkirc is the reference implementation;
  the app consumes the same generic `receipt_pub` pipe in a follow-up
  change that copies this logic.
- `Privmsg` payload changes (`uid`/dedup field) — deferred (see
  Alternatives Considered). No content serialization changes in this
  change.
- Status for `StaticPut`, IRC-surface receipt display, read-by-recipient
  semantics, changes to strike/flood policing or the relay path.

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
| `validate_new` / structural / RLN / parent-fetch failure | `Nack { Invalid }` |
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

### D4: darkirc outbound table

New kvdb tree `darkirc_outbound`, key = event id, value (serial):
`{ event_id, plaintext Privmsg fields, created_ts, state:
Pending|Delivered|Failed, attempts: u16, last_broadcast_ts,
superseded_by: Option<event_id> }`. Written in `publish_events` before
`p2p.broadcast`. Plaintext-at-rest is inside the existing local-wallet
trust boundary (same kvdb that holds RLN identity secrets).

The `superseded_by` chain is the sender-side correlation handle: when a
record is recreated, the new event id links back to the old, so clients
can map statuses for any generation onto one logical message without
any wire-level dedup field.

### D5: Delivery monitor (darkirc)

One task on `IrcServer` (started alongside the client loop):

- Subscribes `receipt_pub`; per (event_id, channel-address) aggregates
  replies; any `Has` (live or historical) closes the record as
  Delivered.
- Ancestry references count as positive evidence: when a new foreign
  event is observed whose ancestry includes a tracked outbound event
  (local walk via the existing `get_ancestors` machinery), the record
  closes as Delivered. Zero wire cost; works among mixed-version peers.
- Periodic sweep (30 s) over `Pending` records, gated by
  `is_synced() && connection_count >= K` (K = 2, both session directions
  counted via the existing session APIs).
- No evidence → rebroadcast the original `EventPut` unchanged (event and
  blob refetched from the local DAG + `dag_blob_fetch`), backoff
  doubling from 30 s, at most `R_MAX` (= 5) rounds per window, then
  keep rebroadcasting at the slow rate.

**Recreate only on explicit evidence (the complete trigger set):**

1. *Window closed, no holder:* the rotation window for the record's slot
   has closed (known locally from the rotation schedule, or indicated by
   `TooOld` nacks) and no peer answered `Has` in the rebroadcast round →
   recreate. This covers: offline across rotation (the canonical case),
   sent into the void while "synced" with zero peers, and clock-skew
   `TooOld` while we believed the window open.
2. *Explicit rejection while open:* every observed reply is a nack and
   at least one is `Invalid` → recreate (bounded by `A_MAX`; reasons
   surface to the user).

Never recreates on: silence (open window — this is what prevents
mass-duplication during mixed-version rollout), `NotSynced`, `Busy`,
or any `Has`. Silence after window close, with no status reply ever
received from any status-capable peer, still recreates — local rotation
knowledge is explicit evidence, and the alternative (gray forever,
possibly lost) is worse than a rare duplicate for old-version holders.

### D6: Recreate procedure

Reuse the existing send path internals: stored plaintext →
`try_encrypt` (fresh nonce — never reuse the old ciphertext) →
`Event::new` (fresh tips/timestamp) → RLN branch:
`reserve_rln_message_id` → `BudgetExhausted` parks the record until
epoch rollover; `MissingIdentity` closes it as Failed → `create_signal`
→ `insert_signal_with_blob` → broadcast → write the new record with
`superseded_by` pointing at the old; `attempts` is shared across the
chain and caps at `A_MAX` (= 2), then Failed + client notice (IRC error
reply to the connected client).

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
shrinks to: mixed-version rollout windows (old peers can't answer
`Has`), adversarial fake-nack griefing, and recreate-of-recreate — all
rare and cosmetic. Asymmetry decides: adding a payload field later is an
additive `Privmsg.version` bump; removing one after shipping is a wire
break. Revisit on field evidence of annoying duplicates.

**Hold-at-send (don't broadcast with zero connections).** Candidate
follow-up: gate `publish_events` on connection count so doomed events
aren't created at all. Not required for correctness (D5 handles them);
deferred as a small polish item.

## Risks / Trade-offs

- [Fake statuses: a peer can lie `Has` (suppresses recreate → silent
  loss) or spam `Nack` (forces recreates → budget burn + injected
  duplicates)] → bounded: statuses only count for ids in our own
  outbound table (blake3 ids are unguessable to non-recipients),
  attempts capped at `A_MAX`, and a peer that has the event relays it
  anyway. Total eclipse defeats this — accepted (an eclipsed node has
  larger problems).
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
  nacks, reply loss] → accepted, rare, cosmetic; documented in the
  proposal's non-goals; additive fix available later if needed.
- [Clock skew: slightly-future local timestamps can earn spurious
  `TooOld` nacks] → worst case is a harmless recreate.
- [RLN interplay: recreate with a stale slot would self-slash] →
  impossible by construction: recreations go through
  `reserve_rln_message_id`, parking on exhaustion, never reusing a
  reserved slot.
- [Plaintext messages persisted in kvdb] → same trust boundary as the
  existing wallet/RLN secrets; tree is local-only.

## Migration Plan

Additive wire message first (`src/event_graph`), verified against a
mixed-version two-node test; then darkirc table+monitor. No content
serialization changes, so no coordination with `darkirc-mod` is
required. Rollback: the message and records are inert for old code;
reverting leaves a harmless `darkirc_outbound` tree. App integration is
a follow-up change that reuses the same `receipt_pub` pipe and copies
the darkirc monitor logic.

## Open Questions

- Exact values of K, `R_MAX`, `A_MAX`, sweep interval — tuning consts,
  safe to adjust after rollout.
- Whether taud later reuses the same policy for task events — deferred,
  out of scope.
