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
  recreate; survives restart; safe with RLN on or off.
- Stable wire format (`u8` discriminants), no new panics on untrusted
  input.

**Non-Goals:**

- Pull-based possession verification (`EventReq` challenges) — dropped;
  status replies are the only signal, treated as hints whose worst-case
  lie is bounded by attempt caps and uid dedup.
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
  `darkfi-serial` derive (variant tag + payload). Unknown variant tags
  must decode-fail into a warn-and-drop, not a panic and not a strike.
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
| `timestamp < genesis_ts` | `Nack { TooOld }` |
| `validate_new` / structural / RLN / parent-fetch failure | `Nack { Invalid }` |
| internal insert error after verification | `Nack { Busy }` |
| `insert_verified_signal` success | `Has { inserted: true }` |

`Has{inserted:false}` is load-bearing: after reconnect-and-rebroadcast it
is the dominant positive outcome ("already propagated") and the only
thing distinguishing it from "never arrived". Strike/flood paths keep
current behavior and send nothing. The duplicate check sits before the
flood-window tick, so replying there is free.

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
`{ uid, event_id, plaintext Privmsg fields, created_ts, state:
Pending|Delivered|Failed, attempts: u16, last_broadcast_ts,
superseded_by: Option<event_id> }`. Written in `publish_events` before
`p2p.broadcast`. Plaintext-at-rest is inside the existing local-wallet
trust boundary (same kvdb that holds RLN identity secrets).

`uid`: 16 random bytes from `OsRng` (invariant: CSPRNG, never reused
across logical messages, always reused across recreations of one
message).

### D5: Delivery monitor (darkirc)

One task on `IrcServer` (started alongside the client loop):

- Subscribes `receipt_pub`; per (event_id, channel-address) aggregates
  replies; any `Has` closes the record as Delivered.
- Periodic sweep (30 s) over `Pending` records, gated by
  `is_synced() && connection_count >= K` (K = 2, both session directions
  counted via the existing session APIs).
- No replies → rebroadcast the original `EventPut` unchanged (event and
  blob refetched from the local DAG + `dag_blob_fetch`), backoff
  doubling from 30 s, at most `R_MAX` (= 5) rounds.
- Any `TooOld`, or all-observed-replies-are-nacks, or `R_MAX` exhausted
  → recreate (D6).
- Records older than the current genesis ts are recreated unconditionally
  on the next eligible sweep (local rotation knowledge; no peer reply
  needed).

### D6: Recreate procedure

Reuse the existing send path internals: stored plaintext →
`try_encrypt` (fresh nonce — never reuse the old ciphertext) →
`Event::new` (fresh tips/timestamp) → RLN branch:
`reserve_rln_message_id` → `BudgetExhausted` parks the record until
epoch rollover; `MissingIdentity` closes it as Failed → `create_signal`
→ `insert_signal_with_blob` → broadcast → write the new record with
`superseded_by` and shared `uid`; `attempts` increments per uid and caps
at `A_MAX` (= 2), then Failed + client notice (IRC error reply; the app
reads the record state through its subscription).

Same `uid` across original and recreations is what makes the duplicate
rendering problem disappear for uid-aware clients.

### D7: `Privmsg` version bump + `uid`

`Privmsg.version` exists and is currently always 0. New payloads use
version 1 and carry `uid`. Decoder is version-matched: v0 decodes
without uid (treated as untracked/legacy), v1 requires it. Old peers
that cannot decode a v1 payload simply fail content-deserialization and
skip the event (existing behavior on undecodable content) — their DAG
still carries it. This serialization change must be sequenced/merged
with `darkirc-mod`'s content tag byte (same struct, same wire).

### D8: App integration

`bin/app` `handle_send` gains the same record-writing and rebroadcast
hooks (its path is RLN-disabled, empty blob). Message identity for
display dedup switches from the ciphertext-hash `msg_id()` to `uid`
(foreign v0 messages keep a synthetic hash id). A subscription task maps
`receipt_pub` events onto per-uid state (`sending`/`delivered`/`failed`)
and notifies the UI; relayed (foreign) statuses are ignored except
optionally as network telemetry.

## Risks / Trade-offs

- [Fake statuses: a peer can lie `Has` (suppresses recreate → silent
  loss) or spam `Nack` (forces recreates → budget burn)] → bounded:
  statuses only count for ids in our own outbound table (blake3 ids are
  unguessable to non-recipients), attempts capped at `A_MAX`, duplicates
  deduped by uid, and a peer that has the event relays it anyway. Total
  eclipse defeats this — accepted (an eclipsed node has larger
  problems).
- [Reply amplification: one reply per relay edge] → tiny fixed-size
  unicast, traffic is RLN-rate-limited anyway, and the flood window is
  untouched (replies are not EventPuts).
- [Unknown-message compatibility: peers without `EventPutStatus`
  receive an unsolicited message type] → verify the channel's behavior
  on unknown message ids during implementation (test with a mixed
  version pair); if unknown ids can destabilize old channels, gate
  replies until a capability flag exists. First implementation task
  resolves this.
- [Mixed-version rollout: old clients render a recreation as a
  duplicate message] → transitional only; documented; uid clients are
  unaffected.
- [Clock skew: slightly-future local timestamps can earn spurious
  `TooOld` nacks] → worst case is a harmless recreate (uid dedup).
- [RLN interplay: recreate with a stale slot would self-slash] →
  impossible by construction: recreations go through
  `reserve_rln_message_id`, parking on exhaustion, never reusing a
  reserved slot.
- [Plaintext messages persisted in kvdb] → same trust boundary as the
  existing wallet/RLN secrets; tree is local-only.

## Migration Plan

Additive wire message first (`src/event_graph`), verified against a
mixed-version two-node test; then darkirc table+monitor; then the
`Privmsg` version bump (coordinated with `darkirc-mod`); then app UI.
Rollback: the message and records are inert for old code; reverting
leaves a harmless `darkirc_outbound` tree. The `Privmsg` version bump,
once shipped, is wire-irreversible — hence sequencing it last.

## Open Questions

- Exact values of K, `R_MAX`, `A_MAX`, sweep interval — tuning consts,
  safe to adjust after rollout.
- Whether `bin/app`'s outbound records live in its app db or a dedicated
  tree — implementation convenience, no behavioral impact.
- Whether taud later reuses the same policy for task events — deferred,
  out of scope.
