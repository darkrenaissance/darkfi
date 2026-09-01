## Context

The event graph (`src/event_graph/`) runs two DAG families: rotating hourly
DAGs carrying darkirc `Privmsg` traffic, and a single never-pruned static DAG
whose admission path (`handle_static_put` → `rln_verify_static_event`)
currently interprets every event's content as an RLN node when RLN is enabled,
and accepts anything structurally valid when RLN is disabled
(`commit_static_event_unverified`). The static DAG already syncs on every node,
including the app (which runs RLN disabled). `Privmsg` has no signature fields;
identity on the wire is limited to the (unauthenticated) nick. `irc2`
(bin/darkirc, embedded by bin/app) has a working IRC-service precedent in
NickServ, and `chanserv` is already a reserved nick in the relay path.

## Goals / Non-Goals

**Goals:**

- Carry channel metadata in the existing static DAG with a content-type
  discrimination scheme that keeps RLN semantics byte-for-byte intact.
- Deterministic, self-certifying channel ownership without consensus changes.
- Client-side policy enforcement (hide, never delete) covering spam-driven
  signal-to-noise loss; flooding stays RLN's problem.
- Owner/admin flows usable from desktop darkirc via ChanServ; app is a policy
  consumer with enable/disable toggles.

**Non-Goals:**

- Encrypted-channel and DM moderation (privacy design needed; follow-up).
- User-installed policies (data model must not preclude them).
- Any change to RLN circuits, proofs, identity tree semantics, or rate limits.
- Server-side message rejection: unsigned/disallowed messages always
  propagate; policy only affects local rendering.

## Decisions

### D1: One static DAG with tagged content, not a second EventGraph

Application payloads ride the existing static DAG. Content is discriminated by
its first byte:

```
static content first byte:
  0x00 | 0x01  → RLN payload (existing RLNNode encoding, unchanged)
  0x02..       → application payload tag (registry below)
```

Rationale: a second EventGraph instance would collide on the fixed p2p message
type names and the hardcoded `"static-dag"` DAG name used by `static_sync`,
requiring invasive protocol namespacing for no benefit. The static DAG's
canonical `(layer, event_id)` ordering — which RLN historical-root consistency
depends on — is content-agnostic, so interleaving app events does not disturb
it. `rebuild_rln_state_from_static` skips app-tagged content deterministically.

Collision note: `darkfi-serial` encodes enum variants as `u64` LE, so
`RLNNode::Registration` starts `0x00` and `RLNNode::Slashing` starts `0x01`.
Reserving exactly those two first bytes for RLN is therefore precise today;
if `RLNNode` ever gains variants, its variant space and the app tag registry
must be reconciled in the same change (assert this in a test). The RLNNode
encoding itself is NOT changed: deployed static DAGs hold RLN events in this
form and they must keep parsing.

Version skew: pre-change nodes fail `RLNNode` deserialization on app-tagged
content (variant index reads as garbage ≥ 2) and skip it without penalty; they
do not relay it. Mixed-version networks are unsupported (see Migration Plan).

### D2: Rotating-DAG content tag byte

Darkirc rotating content gets a leading tag byte; relay paths dispatch on it
instead of trial deserialization. This is a **hard break** of the chat wire
format: there is no untagged form.

```
0x00  Privmsg     (existing fields + optional signer pk + schnorr signature)
0x01  HideAction
0x02+ reserved for future types; unknown tags are skipped, not errors
```

The tag byte is the sole content-type discriminator; the Privmsg struct keeps
its internal fields (`version`, `msg_type`, channel, nick, msg) unchanged and
the signature fields are part of the Privmsg payload itself.

The tag identifies content **type only**. It must not distinguish
encrypted-channel from encrypted-DM payloads: that distinction is metadata an
observer of the public DAG must not get for free. Plaintext vs encrypted is
already self-evident on the wire (plaintext is readable), so a plaintext tag
leaks nothing new; the encrypted forms share one path and resolution between
channel-key and DM-key decryption remains a local trial.

### D3: Per-channel owner chains for ordered state

The static DAG provides replication but no total order. Ordering comes from a
self-certifying per-channel chain:

```
REGISTER #chan {owner_pk}         prev: ∅
  └─► POLICY LIST [...]           prev: <prev static event id>   sig: owner_pk
        └─► TRANSFER {pk_new}     prev: ...                      sig: owner_pk
              └─► POLICY / PIN    prev: ...                      sig: pk_new
```

- Every action names the previous action's **static DAG event id** and is
  signed by the current owner key (schnorr, pallas).
- Resolution: winning registration (first in canonical order among valid
  registrations of the same name) + longest chain of validly linked, validly
  signed actions; canonical `(layer, event_id)` order as tie-break.
- Signature covers channel name, action payload, and prev link — so a valid
  signature also proves chain position intent.

Alternatives considered: last-writer-wins by DAG order (racy — two nodes see
different orders and flip-flop); side-events for owner ops (unnecessary — owner
ops are rare, and chains give transfer-of-authority for free).

### D4: Definitions in the static chain, application in the rotating DAG

- **Static chain actions**: `Register`, `Transfer`, `PolicyList`, `Pin`.
  Authorization data (owner key, admin sets, allow-lists, filter params,
  default-enabled flags) must be durable and ordered → chain.
- **Rotating events**: `HideAction { channel, target_event_id, hidden, actor_pk,
  sig }`. Hides are frequent, need no ordering beyond last-wins-per-target in
  canonical rotating order, and gain two properties by expiring with the
  window: no permanent public record that an event existed/was hidden, and no
  static-DAG churn. The chain's admin set authorizes them; an action signed by
  a key outside the current (enabled) admin set resolves to nothing.

Hide tombstones reference the **DAG event id** (stable, node-independent,
available pre-decryption), not the client-side content msg_id (timestamp-
correction hacks make it unstable). The hide check runs in the relay path where
both ids are in hand, before msg_id conversion.

### D5: Policy model — hardcoded u8 enum, opaque params

```
u8   ChanServ name   params
0    ALLOWLIST       Vec<pk>   render only messages with valid sig from set
1    ADMINHIDE       Vec<pk>   keys may publish HideActions
2    FILTER          rules     regexes over the privmsg (nick and/or msg)
```

The wire format carries the `u8`; the ChanServ command surface accepts the
policy name (`ALLOWLIST`, `ADMINHIDE`, `FILTER`) and parses it to the enum
value, rejecting unknown names.

A `PolicyList` chain action replaces the whole list wholesale; each entry is
`{policy: u8, params: Vec<u8>, enabled: bool}`. Params are opaque bytes owned
by the built-in evaluator for that id; unknown ids are ignored at resolution
(forward compatibility, also the seam where user-installed policies land
later). The enum numbering is frozen at merge; adding policies appends.

`FILTER` evaluation happens on the decoded plaintext privmsg (after
channel/DM decryption for encrypted targets — same local trial as rendering).
Regex compilation from policy params is fallible: an uncompilable rule is
ignored and the remaining rules still apply (untrusted-input invariant: policy
params are owner-controlled but owners can be adversarial). The evaluator
uses a linear-time regex engine (Rust `regex` crate: finite-automata, no
catastrophic backtracking) so hostile patterns cannot wedge a client.
Note: `regex` is already a `bin/app` dependency but is a new dependency for
`irc2` (`bin/darkirc/Cargo.toml`) — dependency addition requires human review
per repo policy.

### D6: Privmsg signing

The Privmsg payload keeps its existing field order and appends optional
`signer_pk` + `sig`. The signature is over the serialized core fields
(`version | msg_type | channel | nick | msg`), computed on plaintext before
any channel/DM encryption. For encrypted targets the sig fields are encrypted
along with the rest (nothing visible outside); for plaintext channels they are
public. Signatures do not cover the header timestamp — client-side timestamp
correction must not break verification, and content replays under a new event
id are indistinguishable from quotes, which IRC legitimately allows.

Posting in an allow-listed channel is **opt-in linkability**: the policy only
makes sense where users accept a persistent per-channel signing key. This is
stated UX, not a hidden cost.

### D7: Pins snapshot and re-encrypt

`Pin { target_event_id, snapshot }` is an owner chain action. The snapshot
embeds the message content (nick, msg, original timestamp) because the pinned
event expires from the rotating window after 24h. For saltbox channels the
snapshot is encrypted under the channel key — the static DAG must never carry
plaintext of an encrypted channel. FUD icon links etc. are future chain action
kinds; the action enum is extensible.

### D8: Static app-event admission bounds and sync

- Admission: existing structural checks + a content size bound (order of a few
  KiB, exact constant at implementation) for app-tagged static events, applied
  identically in both RLN modes. `handle_static_put`'s existing moving-window
  flood guard covers the live path.
- `static_sync` currently requires a non-empty blob per non-genesis event (for
  RLN proof re-verification). App-tagged events carry no proof: the blob
  alignment check must treat empty blobs as valid for app tags. Structural
  checks (parents present, content-hash match, size bound) are re-applied at
  sync; signature/chain validity is resolved above the sync layer.
- Everything parsing untrusted bytes stays fallible (no unwraps); malformed
  app events are skipped, never fatal.

### D9: ChanServ

Mirrors NickServ: NOTICE replies, HELP texts, command dispatch from
`handle_cmd_privmsg` on the reserved `ChanServ` nick. Authentication is
possession of the locally configured key: ChanServ signs with the config's
owner (or admin) key and refuses commands whose matching key is absent.

```
/msg ChanServ REGISTER #channel
/msg ChanServ INFO [#channel]          owner, chain tip, policy list, pins
/msg ChanServ TRANSFER #channel <pk>
/msg ChanServ POLICY #channel LIST
/msg ChanServ POLICY #channel SET <name> <params>     e.g. SET ADMINHIDE <pk1>,<pk2>
/msg ChanServ POLICY #channel DEFAULT <name> ON|OFF
/msg ChanServ PIN #channel <event_id>
/msg ChanServ UNPIN #channel <event_id>
/msg ChanServ HIDE #channel <event_id>                (admin key)
/msg ChanServ UNHIDE #channel <event_id>              (admin key)
/msg ChanServ HELP [command]
```

Usage examples with real values, one per policy type:

```
--- ALLOWLIST (policy 0): only signed messages from these keys render ---

/msg ChanServ POLICY #darkfi SET ALLOWLIST 9mkH5rwnYtV4JCvfH2N7yc6bT1eSQkWLDpGXzKR8uFq3,7dRm2cVbXwNZtPjLKe84TghY6FqsaU1JzCNoEWBkv5Py
/msg ChanServ POLICY #darkfi DEFAULT ALLOWLIST ON
   → the channel now defaults to rendering only messages signed by
     one of the two listed keys; senders attach their pk + signature

--- ADMINHIDE (policy 1): these keys may hide/unhide messages ---

/msg ChanServ POLICY #darkfi SET ADMINHIDE FgYU9dV1K2vQmTHeqXPnWZuLycA4sBDr7EJk6atMZiWo
/msg ChanServ HIDE #darkfi 9a41c7e0d3b8f6521e0a4cd7b83f19e2c5a6d0b8f3e2714c9d5a8b6e3f0c2d17
   → the admin-key holder hides the spam event; clients mark it hidden
/msg ChanServ UNHIDE #darkfi 9a41c7e0d3b8f6521e0a4cd7b83f19e2c5a6d0b8f3e2714c9d5a8b6e3f0c2d17
   → restores it if hidden in error

--- FILTER (policy 2): rules matching nick and/or message content ---

/msg ChanServ POLICY #darkfi SET FILTER nick:^spam\w*$,nick:^ninja\d+$,msg:(?i)(airdrop|free coins|giveaway)
/msg ChanServ POLICY #darkfi DEFAULT FILTER ON
   → each rule is field:regex, comma-separated; nick rules match the
     sender nick, msg rules match the message body; matching messages
     are hidden for clients applying the policy
```

Public keys are base58-encoded pallas points (schnorr public keys); event ids
are the blake3 DAG event ids shown by `INFO` or by the future app
context-action. `HIDE`/`UNHIDE` take a pasted event id for now; the app later
adds a context-menu action emitting the same underlying flow.

### D10: Key provisioning

Schnorr keypairs (pallas) configured as secrets in darkirc TOML (desktop) and
the app settings store; public keys derived, never configured separately.
Secrets are used only for local signing and are never logged or transmitted
(hard invariant: no secret leakage).

### D11: App integration — clickable channel label and policy overlay

In the app, entering a channel from the menu shows the chat screen with the
channel name (e.g. `#dev`) as a label at the top. That channel-name label
becomes tappable and opens the per-channel policy overlay:

```
chat screen                          overlay (modal layer)
┌───────────────────────────┐        ┌────────────────────────────────┐
│ [#dev]  ← channel label   │   tap  │ #dev — channel policy         │
│───────────────────────────│  ───►  │ ──────────────────────────────│
│ 12:01 <alice> hey all     │        │ [x] AllowList      (default)  │
│ 12:02 <bob>  ...          │        │ [x] AdminHide      (default)  │
│        ...                │        │ [ ] FILTER         (default)  │
└───────────────────────────┘        │  overridden locally: ON       │
                                     └────────────────────────────────┘
```

What changes in the app:

- **Channel-label tap target** (`bin/app/src/app/schema/chat.rs`): the chat
  screen already places the channel-name label at fixed coordinates
  (`CHANNEL_LABEL_X`/`CHANNEL_LABEL_Y` and friends). The tap target is a
  normal button node placed on top of that existing label — same button
  pattern the chat screen already uses for send/emoji buttons — whose
  activation opens the policy overlay for the displayed channel. No changes
  to the chatview text rendering or hit-testing are required.
- **Policy overlay**: a new overlay scene node (following the app's existing
  overlay/layer patterns) fed by the resolved channel policy state from the
  plugin cache (task group 6). It lists the owner's current default policy
  list — one row per policy with the default state — plus toggle switches.
  Toggling writes/updates the user's override row in the local table and
  triggers a re-filter of that channel's buffer; because hidden messages are
  marked rather than dropped (D4), re-filtering is a pure view update with no
  refetch. Rows reflect `default || override` resolution: the overlay shows
  both the owner default and the user's effective state.
- **Unregistered channels**: a channel with no resolved registration shows an
  empty/ informational overlay ("no owner policy"), leaving room for future
  user-local policy rows in the same table.
- This is deliberately a latter stage (after the plugin cache, override
  table, and evaluators land) so the overlay only wires together existing
  resolved state.

## Risks / Trade-offs

- [Static app-event flooding: no proof gates publication, and the static DAG
  is never pruned] → size bound + existing moving-window peer flood guard;
  resolution cost of garbage is one signature check; monitor growth. If it
  becomes abuse, gate app static events behind stake/registration in a future
  change.
- [Touching the RLN admission path (`handle_static_put`, `static_sync`,
  rebuild) is security-critical] → RLN branches stay byte-identical; new code
  is additive dispatch on first byte; dedicated tests assert RLN behavior
  unchanged; human review on the diff (per repo policy).
- [Mixed-version networks: pre-change nodes cannot decode tagged rotating
  content and skip app static payloads] → unsupported state by decision: the
  chat wire format is a hard break and all darkirc nodes upgrade together;
  rotating content self-drains within one rotation window (≤24h); static RLN
  encoding is unchanged so RLN state survives the upgrade untouched.
- [Owner key loss bricks the channel chain] → inherent to key-based ownership;
  documented; transfer is the only recovery path while the key exists.
- [Hide actions are censorable by policy-off clients — moderation is advisory]
  → by design: policy is local preference, not network consensus.
- [Signing into allow-listed channels is linkable across messages] → explicit
  UX tradeoff (D6), per-channel keys mitigate.

## Migration Plan

Hard break of the darkirc chat wire format; no compat shims and no legacy
untagged Privmsg. Deploy: all darkirc nodes (desktop + app) upgrade in one
coordinated step. Existing untagged messages in rotating DAGs become
unreadable but age out permanently within one rotation window (≤24h) — no
data migration, the network self-cleans. Static-DAG RLN history remains valid
(the RLNNode encoding is untouched); application static payloads are new and
only exist post-upgrade. Rollback = redeploy pre-change binaries and let the
window drain again. After merge, run `make` (proofs/contracts unaffected) and
`make test`.

## Open Questions

- Exact static app-content size bound (few KiB; fix during implementation).
- Whether `UNHIDE` is a separate command or `HIDE` with a flag (command
  surface only; resolution semantics already fixed as last-wins-per-target).
- App-side representation of the resolved-policy cache (in-memory only vs
  persisted); does not affect wire or resolution semantics.
