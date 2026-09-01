# Design: swarm overlay for swarm rendezvous

## Context

See `proposal.md` for motivation and delta specs for normative behavior. Current
implementation constraints are:

- One DarkFi network is one `P2p`, isolated by magic bytes and an app-name plus
  major/minor handshake. Host state, sessions, registry, and persistence belong
  to that instance.
- `Settings.seeds` uses short-lived `SESSION_SEED`; it cannot carry swarm
  queries. `Settings.peers` creates ordinary manual channels.
- Current manual peers carry only `Url` and transports resolve internally.
  Exact cached sockets therefore require an explicit pre-start manual-target
  API and connector path; they cannot be represented by `Settings.peers`.
- `ManualSession::reload()`, `SeedSyncSession::reload()`, and
  `InboundSession::reload()` currently do not reconcile changed addresses.
  Bootstrap fallback, source fallback, and serving promotion cannot depend on
  reload.
- `Hosts::subscribe_channel()` publishes completed seed/refinement channels as
  well as ordinary channels. Join completion must filter session type.
- `VersionMessage.features` is retained remotely but sent locally as empty.
  Existing variable version fields can combine past `VERSION_MAX_BYTES`, so
  feature validation alone is insufficient.
- An ad store knows only one-way `SwarmId` and addresses. It cannot perform a
  swarm handshake and must not become an attacker-controlled dialer.
- Current Tor state is process-global and does not guarantee independent onion
  identities per swarm; I2P does not provide a general inbound listener.
- Apps such as darkirc/fud construct substantial state from `P2pPtr` before
  protocol registration. A registration closure alone is insufficient.

## Terminology

One sense per word:

| Term | Meaning |
|---|---|
| overlay | the single rendezvous `P2p` network (fixed identity `darkfi-swarm`) where ads and lookups happen |
| swarm | one application network: one descriptor, one `SwarmId`, one independent `P2p` instance |
| `SwarmPool` | the orchestrator owning at most one overlay plus the independent `P2p` instance of each active swarm |

The naming follows BitTorrent, which uses the same shape:

| BitTorrent | Swarm subsystem |
|---|---|
| swarm = all peers sharing one torrent | swarm = the peers of one application network |
| tracker / DHT rendezvous | the overlay, which hands out swarm addresses |
| client session (libtorrent `session`) | `SwarmPool` |

Wire names use the per-network sense: `getaddr`/`addrs` fetch one swarm's
addresses and `getswarm`/`swarms` enumerate public swarms. `SwarmError`,
`ProtocolSwarm`, the `swarm` feature and `net::swarm` module, and
`swarm-ad-store` name the overlay protocol and subsystem, never a single
swarm.


## Goals / Non-Goals

**Goals:**

- Reuse ordinary `P2p` instances without changing framing, compatibility, or
  seed-session semantics.
- Keep all untrusted wire, persistence, queue, request, and work state bounded.
- Make bootstrap/source attempts, join completion, serving creation, rollback,
  recreation, and teardown explicit and testable.
- Store/relay hints passively and validate only inside a joining swarm.
- Add no third-party dependency.
- State realistic protocol disclosure and persistence boundaries.

**Non-Goals:**

- Multiplexing swarm traffic over overlay channels.
- Authenticating ad authors or proving address ownership.
- PIR, cover traffic, Sybil resistance, or global-observer resistance.
- Automatic independent Tor/I2P provisioning.
- Runtime reconciliation of manual, seed, or inbound session settings.

## Decisions

### D1. Isolated `net::swarm` module and feature

```text
src/net/swarm/
├── mod.rs
├── settings.rs
├── descriptor.rs
├── message.rs
├── protocol.rs
├── store.rs
├── bootstrap.rs
└── lifecycle.rs
```

`src/net/mod.rs` exposes the module only with `feature = "swarm"`. The feature
enables existing `net`, `blake3`, `kvdb-overlay`, and serialization facilities.
Lilith and the pilot opt in explicitly. Any newly required dependency,
`build.rs`, or proc-macro stops implementation for human supply-chain review.

Nested placement gives `darkfi::net::swarm` and narrow crate-private host access
without making orchestration core `P2p` behavior. A top-level module would need
more public helper surface and a separate network-dependent root.

### D2. Fixed overlay identity

All overlay instances use:

```text
app_name:     "darkfi-swarm"
app_version:  1.0.0
magic_bytes:  [0x78, 0x85, 0xa4, 0x2a]
```

The magic is the first four bytes of
`BLAKE3("darkfi-swarm-overlay-v1")`. Callers cannot override these fields.
Future incompatible overlay changes follow existing major/minor rules.

### D3. Manual canonical descriptor encoding

`descriptor.rs` writes the exact spec bytes with checked lengths and explicit
big-endian integers; it does not depend on general serializer stability.
Application names are restricted to 32 UTF-8 bytes to remain valid in both
version and verack bounds. Private construction accepts `[u8; 32]`, while
generation fills it from `OsRng`. The golden vector is tested before any app pin
is accepted.

String concatenation and generic struct serialization are rejected because
field/format ambiguity would split deployed IDs.

### D4. Correlated bounded wire protocol

Initial messages are:

```text
SwarmAd {
    swarm_id, visibility, ad_id: [u8; 32],
    lifetime_secs, addrs: Vec<Url> // 1..=32
}
GetSwarmAddrs { request_id: [u8; 16], swarm_id, cursor? }
SwarmAddrs    { request_id: [u8; 16], swarm_id, addrs, next? }
GetPublicSwarms { request_id: [u8; 16], cursor? }
PublicSwarms    { request_id: [u8; 16], swarm_ids, next? }
SwarmError       { request_id: [u8; 16], bounded_code }
```

Commands are fixed to `ad`, `getaddr`, `addrs`, `getswarm`, `swarms`, and
`err` respectively. Struct field order is exactly the order shown in
`swarm-overlay`; existing DarkFi encoding is used. Visibility is
`u8` (`0` public, `1` non-public), lifetime is `u32`, error codes are fixed
`u8` values 0 through 3, and cursor version is one. No new serializer is added.

Request IDs come from `OsRng`; a per-channel map permits at most 32 pending
requests and removes entries on response, disconnect, or the default 10-second
timeout (configurable to at most 60 seconds). Timeout is local; late, unknown,
duplicate, or wrong-type responses are unsolicited and penalized.

Every URL is at most 1,024 encoded bytes. Message maxima are fixed as in the
spec: ad and address response 65,536; address/public requests 128; public
response 16,384; error 128. Count and byte validation precede store/work.

All swarm, version, and verack decoders are audited as attacker-input paths.
They use checked reads and return errors for truncation/invalid structure; no
`unwrap`, `expect`, explicit panic, unchecked slice/index, or allocation from an
unvalidated declared size is permitted. Tests truncate valid payloads at every
byte and feed hostile-length/arbitrary payloads under unwind and allocation
instrumentation.

`PageCursor` is fixed 65 bytes:

```text
version: u8 | last_key: [u8; 32] | terminal_key: [u8; 32]
```

Address pages use BLAKE3 of canonical URL bytes as ordered key; public pages use
`SwarmId`. The first page captures the greatest current live key as a terminal.
Later pages return live keys strictly after `last_key` and no greater than that
terminal, then advance `last_key`. Mutation may make a traversal include or omit
records, but never invalidates a well-formed cursor, allocates a server snapshot,
or extends traversal beyond its initial terminal. This trades snapshot
consistency for bounded progress under adversarial mutation.

Responses derive canonical item keys and require strict ascending uniqueness in
the cursor window. `next.last_key` equals the greatest returned key; empty pages
have no next cursor. Requesters independently derive keys, retain a bounded seen
set, and reject within/across-page duplicates, unordered/out-of-window items,
changed terminals, non-advancing cursors, or cursor/item disagreement.

Public enumeration is disabled by default. It indexes IDs with at least one live
normalized address record marked public and is available to all connected
protocol-correct peers when enabled. Direct lookup ignores visibility. Because
visibility is unsigned, an attacker can re-advertise an observed ID as public.

### D5. Passive store with protected replay admission

Persistent nodes use existing `kvdb-overlay` trees for address records, public
index, seen IDs, and monotonic-epoch metadata. Address keys are
`(SwarmId, hash(canonical_url))`. Atomic batches update records and indexes.
Seen keys remain global by `ad_id`; their values bind the advertised `SwarmId`
and general/local-reserve class only for quota/replay accounting. Reusing one ad
ID under another swarm is therefore still a duplicate. No persistence API
receives a source channel/address.

Each key has one normalized record containing URL, current visibility, expiry,
and accepting ad ID. A fresh ad atomically updates records for its included
addresses; absent addresses remain until separately updated/expired/evicted.
The public index contains an ID iff at least one live record is public, so a
same-address visibility update can add/remove catalog membership while mixed
records keep it public. Direct lookup reads all live records. Restart rebuilds
or verifies the public index from normalized records.

Defaults and hard maxima are:

| Local limit | Default | Maximum |
|---|---:|---:|
| addresses per swarm | 256 | 1,024 |
| total addresses | 16,384 | 65,536 |
| general protected IDs per swarm | 256 | 1,024 |
| protected ad IDs | 65,536 | 262,144 |
| local-author reserve swarm partitions | 32 | 256 |
| accepted/authored address lifetime | 7,200 s | 86,400 s |
| replay checkpoint interval | 300 s | 600 s |
| relay fanout | 16 | 64 |
| bootstrap-stage timeout | 30 s | 300 s |
| complete join timeout | 120 s | 900 s |

The local expiry for each accepted address is the lesser of wire lifetime and
the configured receive cap. Local author lifetime uses the same default and hard
maximum. Relay preserves the original validated wire lifetime; each receiver
applies its own cap.

Address capacity evicts expired first, then earliest expiry, then lexical key.
Seen IDs remain protected exactly through local address expiry plus 86,400
seconds, for at most 172,800 seconds from acceptance. Expired IDs
are removed first. Remote IDs occupy a general pool with a per-swarm quota; if
that quota or the global general pool contains only protected IDs, the fresh ad
is rejected before address mutation or relay. This prevents one claimed swarm
from consuming the whole general pool, but generated swarm IDs can still cause
distributed saturation.

Authoring configuration reserves a default 32, at most 256, swarm partitions of
256 slots each inside the global cap. Checked multiplication/subtraction derives
reserve and nonzero general capacities. Remote ads cannot consume a partition;
locally authored IDs use their swarm's partition until expiry. A serving
transition atomically allocates/reuses a partition before listener/author start;
stopping retains it until every protected local ID expires. Sequential churn may
therefore return a typed capacity failure rather than overwrite protection. The
last-ID expiry releases a stopped swarm's partition atomically; resumed serving
retains it.

Startup assigns persisted local IDs to partitions by distinct swarm and checks
each partition's 256 slots separately. Persisted general IDs are checked only
against the remaining general capacity and per-swarm quota; local IDs already
inside reserve are not double-counted. The 256-slot partition exceeds the
maximum IDs produced by the fixed 20-minute minimum cadence during the
172,800-second maximum protection window. Protected IDs are never evicted early;
replay semantics still take priority over general remote-ad availability.

Reserve records necessarily identify to the local store which ephemeral ad IDs
this process generated. They contain no peer/source address or stable author
identity; reserve occupancy/use/failure/timing and local-origin classification
are excluded from wire, RPC, status, metrics, and telemetry, including
aggregate counters. This local-only authorship fact is an explicit cost of
preventing remote saturation from blocking the process's own cadence.

Ad acceptance atomically commits the global seen-ID record, pool accounting,
address records, and public index before relay enqueue. A failed commit causes no
mutation or relay. Restoring a database snapshot from before this commit removes
the seen ID and can permit replay; rollback-resistant replay suppression would
require external non-rollbackable state and is not claimed.

Runtime expiry uses `Instant`. Persistence records accepted wall time, absolute
expiry, original lifetime, and last-observed store wall time. Address records
restore remaining time, clamped by local/original/protocol lifetime; rollback
may expire addresses conservatively.

Seen IDs instead use unsigned 64-bit seconds on a durable monotonic epoch. One
metadata value atomically checkpoints elapsed ticks every 300 seconds by default,
at most every 600 seconds, and on clean shutdown. Restart compares before
subtracting: `deadline <= checkpoint` expires; otherwise checked subtraction must
produce at most 173,400 seconds, valid remainder is clamped to 172,800, and
checked duration conversion/`Instant::checked_add` builds the new deadline.
Underflow, overflow, larger delta, or missing/incoherent metadata is a typed
startup error.

A checkpoint write that cannot complete before 600 seconds places persistent
admission/authoring in fail-closed mode until checkpoint recovery or controlled
shutdown; bounded reads may continue. This prevents new deadlines from exceeding
the maximum validated delta.

All surviving records and the new epoch replace the prior epoch atomically;
interruption leaves the old epoch loadable. Uncheckpointed run time and downtime
are not subtracted, so they can extend a record present in the loaded database.
Repeated restart does not reset it to a fresh full horizon. Rollback before ID
commit can remove the record entirely and is outside this guarantee. General and
reserve partitions are validated independently before conversion.

Transient stores use the same validation in bounded memory only. No store
contains an active dialer. Active refinement is rejected because it cannot
verify swarm attribution and creates scanning amplification.

### D6. Validate features and full version size

`net::Settings` gains a bounded local feature vector, empty by default.

Feature validation rejects duplicate/overlong names, excess count, and invalid
versions. Version permits at most 10 external addresses and 10 features;
node ID is capped at 64 bytes, app name at 32, URL at 1,024, feature name at 32,
and semver prerelease/build at 32 each. Before sending, protocol encodes and
checks complete `VersionMessage` and `VerackMessage` against their maxima.
Each overlay/swarm `P2p` receives a fresh CSPRNG node ID scoped to that instance;
it is not persisted or reused across networks/restarts.

Inbound `VersionMessage` and `VerackMessage` use manual bounded decoders that
preserve existing field order and bytes while reading every declared
string/vector length before reservation. They reject over-limit node/app and
semver prerelease/build strings, external-address count/URL lengths, feature
count, and feature-name length before `try_reserve` or allocation. Golden tests
compare custom encoding/decoding with existing valid wire vectors.
Compatibility rules remain unchanged.

### D7. Protocol registration and independent work accounting

`ProtocolSwarm` registers on `SESSION_DEFAULT` only: ordinary inbound,
outbound, manual, and direct channels, excluding seed/refinement. Per-channel
instances share store, work limiter, and bounded relay queue.

Generic message metering is supplemented by token buckets keyed by ephemeral
channel ID for validation/write, query/response bytes, pending/cursor work, and
relay enqueue. Global semaphores bound durable writes, reads, and relay jobs.
Channel accounting is removed on disconnect and never keyed by peer address.

Protocol rates per channel are 32 ads, 16 direct requests, 16 direct responses,
4 public-list requests, 4 public-list responses, and 16 errors per 10 seconds;
work rates are 32 store writes and 32 relay enqueues per 10 seconds plus
1,048,576 response bytes per 60 seconds. Initial local defaults/maxima are:

| Resource | Default | Maximum |
|---|---:|---:|
| relay queue | 1,024 | 4,096 |
| concurrent durable writes | 8 | 32 |
| concurrent reads | 16 | 64 |
| relay workers | 8 | 32 |
| pages per direct join lookup | 16 | 16 |
| pages per public enumeration | 4 | 16 |
| candidate addresses per attempt | 64 | 256 |
| previously compatible retries | 16 | 64 |
| persisted compatible retry URLs/swarm | 64 | 256 |
| local-author reserve swarm partitions | 32 | 256 |
| active swarms | 32 | 256 |
| concurrent lifecycle attempts | 8 | 32 |
| shutdown deadline | 120 s | 600 s |
| pending request timeout | 10 s | 60 s |
| configured ordinary peers | 8 | 256 |
| overlay bind addresses | 1 | 16 |
| serving bind addresses/swarm | 1 | 16 |
| serving external addresses/swarm | 1 | 32 |
| overlay inbound channels | 64 | 256 |
| overlay outbound channels | 8 | 64 |
| overlay manual channels | 8 | 256 |
| total overlay channels | 80 | 512 |
| untrusted dial concurrency | 4 | 16 |
| untrusted dial starts/minute | 32 | 128 |
| DNS resolutions/join attempt | 64 | 256 |
| dials/resolved destination/attempt | 1 | 1 |

Each accepted ad queues at most one relay job and sends to no more than fanout
eligible ordinary channels, excluding source by channel ID. Queries answer only
their requesting channel. Overlay uses strict ban policy. This bounds work but
does not claim Sybil resistance.

### D8. Bootstrap by constructing one stage at a time

SwarmPool owns at most one running overlay candidate. A cache record is a bounded
pair of the original connect URL and exact resolved endpoint used by a
successfully completed persistent-feature channel. It never comes from
`VersionMessage.ext_send_addr`. For a transient:

1. Parse the endpoint-only cache under file/count/URL/shareability bounds and
   revalidate each stored socket without DNS.
2. Construct overlay settings with `Settings.peers = []` and
   `Settings.seeds = []`, then install cached targets through the explicit
   pre-start manual-target API.
3. Register protocol, subscribe to channels, start, and wait for a compatible
   ordinary channel.
4. On timeout/failure, fully stop and discard the candidate.
5. Resolve/validate configured ordinary peers once, construct a fresh overlay
   candidate, install those exact targets through the same pre-start API, and
   repeat one bounded stage.
6. Publish the successful `P2pPtr` as SwarmPool's active overlay only after success.

No settings reload is used. Persistent nodes construct directly from configured
ordinary topology. After an ordinary channel exposes `swarm-ad-store`, only its
actual connect/resolved pair may enter the cache via atomic replacement. The
peer's advertised external addresses are ignored for caching.

SwarmPool does not configure a standard overlay hostlist/datastore for a
privacy-maximal transient. Separately configured swarm and transport state is
outside that overlay-cache guarantee and documented.

Untrusted dial paths use a new narrow target model:

```text
ValidatedDialTarget { original_url, exact_socket, route }
route = Direct | TrustedProxy { destination_kind }
ManualSession::add_targets_before_start(Vec<ValidatedDialTarget>)
ManualSession::add_target_plan_before_start({ first, second, switch_at })
Connector::connect_validated(ValidatedDialTarget)
```

The pre-start method creates ordinary manual slots before `P2p::start()` and is
not reload/reconciliation. The two-phase plan preinstalls already validated
targets, activates only `first` at start, cancels it at a monotonic switch time,
then activates `second`; a missing first phase activates second immediately.
Overlay bootstrap uses the one-phase method. Fresh clearnet targets resolve once
and cache their socket; cached targets skip DNS and revalidate their stored
socket. In a direct route, `exact_socket` is the destination socket and the
connector opens that exact socket without re-resolving; `original_url` supplies
only TLS server-name identity.

In a trusted-proxy route, `exact_socket` is the exact locally configured proxy
socket, not an advertised destination. The advertised URL cannot choose or
override it. The destination is restricted to a globally routable IP literal or
a canonical Tor/I2P hidden-service name matching the transport. A hidden name is
never locally DNS-resolved and is passed only inside proxy negotiation and, when
applicable, TLS identity. Arbitrary clearnet hostnames are rejected rather than
remotely resolved. The configured proxy socket may be loopback/private under
local trust policy; that exception never applies to direct advertised targets.
Missing/malformed proxy configuration is a candidate error. Production lilith
forces direct-target local-test mode off.

The full untrusted candidate pipeline—URL parse, allowlisted scheme, host/port,
DNS result handling, address classification, target construction, proxy
selection/negotiation, transport/TLS dial, and compatibility—is fallible and
contains no panic, unchecked indexing, or unimplemented branch. Empty DNS sets,
more than 16 results for one URL, malformed/missing proxy targets, unsupported
or unaudited schemes, timeout, and cancellation return bounded errors. A
bounded nonempty multi-address result is iterated safely, every address consumes
the join resolution budget and is classified, and at most one allowed exact
socket is selected for that URL. Only schemes whose adapters satisfy this rule
are accepted from advertisements; enabled but unaudited transports are rejected
before dialer construction.

Resolution and dialing consume the D7 concurrency/rate/total budgets. A join
attempt tries one resolved destination once. This does not prove endpoint
ownership, but prevents local-network SSRF/DNS-rebinding and bounds public
victim reflection.

### D9. Keep overlay control channels and swarm data channels separate

“Ordinary overlay session” means an inbound/outbound/manual/direct non-seed
session on the overlay `P2p`; it does not mean every client keeps it for process
lifetime. The channel remains bound to overlay magic/app identity, channel
store, hosts, and `ProtocolSwarm`. Streams are never handed to another `P2p`,
re-handshaken under swarm identity, or extended with swarm-tag multiplexing.

Persistent nodes retain the overlay while storing/relaying. Serving nodes retain
it while authoring ads. Transient settings expose two policies:

```text
SessionBound                 // default; retain overlay for application session
ImmediateAfterOperation      // explicit reduced-privacy mode
```

The default never reacts to lookup/join completion by disconnecting; it retains
the overlay until an explicit `stop_overlay()` or full SwarmPool shutdown ends the
application session. Immediate mode deterministically stops after every
caller-visible lookup or join reaches a terminal outcome—success, empty result,
error, timeout, or cancellation—but not after an internal lookup phase within a
join. Its configuration warning states that the responder and a same-operator
swarm server may correlate query, swarm connection, and teardown timing. In
either mode the swarm handle owns an independent `P2p` and outlives overlay
stop. Later discovery runs staged bootstrap again only when no active overlay
remains.

SwarmPool tracks overlay lifetime separately from swarm registry lifetime.
`stop_overlay()` rejects persistent/serving duties, but for an eligible
transient it stops only overlay tasks/channels and leaves swarm entries
untouched. Full SwarmPool shutdown still stops every swarm and any active overlay.
Transport reuse was rejected because it requires multiplexing or handoff,
correlates overlay queries with swarm membership, mixes host/protocol state,
and works only when the overlay peer also serves the swarm.

### D10. Registry-owned lifecycle and explicit source attempts

Registry states are:

```text
Initializing -> Joining -> Joined
Initializing -> Serving
any active state -> Stopping -> Absent
```

One lock owns transitions per ID; different IDs proceed concurrently. Registry
entries retain `P2pPtr`, type-erased `Arc<dyn Any + Send + Sync>` app state, a
shutdown hook, mode, and persistence policy. Caller handles clone the typed
`Arc`; dropping them cannot drop registry ownership.

A join attempt:

1. validates descriptor and reserves `Initializing`;
2. builds namespaced settings and `P2p` for that attempt's sources;
3. subscribes to completed channels before start;
4. runs the fallible initializer, stores app ownership/shutdown, and registers
   protocols before start;
5. loads the complete at-most-256-entry compatible retry index and performs
   bounded `OsRng` reservoir sampling over every valid URL returned by the fresh
   lookup's fixed-terminal traversal through completion or its 16-page cap;
6. independently shuffles both tiers and resolves/validates them under a
   candidate-preparation subdeadline capped at half the then-remaining overall
   time; verified and fresh resolution each receive half that subdeadline, so
   verified DNS/transport preparation cannot consume fresh preparation time;
7. retains selected candidates as ephemeral `ValidatedDialTarget` values and
   installs a two-phase verified/fresh plan through the swarm's pre-start manual-
   target API, not its URL-only hostlist/refinery;
8. starts and waits for a channel whose session flag is inbound, outbound, or
   manual; at start it snapshots remaining dial time, cancels verified targets at
   its monotonic midpoint, and activates fresh targets for the second half,
   explicitly rejecting temporary direct, seed, and refinement notifications;
9. confirms the channel remains an ordinary registered peer, then atomically
   transitions to `Joined`; or
10. on error/timeout/cancel, stops all attempt state and removes ownership.

Failed pre-compatibility targets are dropped and never persisted. A successful
ordinary channel may register its canonical peer URL in normal host state.
Every later outbound retry/refinement resolves and validates a fresh exact
socket under the same egress/rate rules before connection; no path may connect
by reusing a prior validation followed by a second resolver call. Tests count
DNS queries and inspect exact sockets across manual, outbound, retry, refine,
and persisted-host paths. URL, store, hash, and DNS-answer order never selects
the resolution/dial prefix: full bounded persisted state is shuffled, fresh URLs
use reservoir sampling across the terminal traversal through completion or its
16-page cap, and each bounded allowed DNS answer set is CSPRNG-shuffled before
selection. The dedicated retry
index defaults to 64 and never exceeds 256; when full, a newly compatible peer
is usable now but does not evict an existing retry entry merely for admission.
This reduces ordering/grinding bias but cannot force a malicious responder to
return an honest candidate.

Source policies:

- overlay-only: no static seeds;
- static-only: configured seeds, explicitly activated for that attempt;
- combined: both sources are intentionally configured in one attempt; and
- overlay-then-static: complete one overlay-only attempt; on failure stop it,
  then create a fresh static-only `P2p`, rerun initializer, and remain under one
  overall deadline.

This avoids nonfunctional manual/seed reload. Rerunning initialization is an
explicit observable cost and both attempts have independent rollback.

### D11. Serving is initial configuration or controlled recreation

`ServeSettings` separates:

```text
bind_addrs:       Vec<Url> // local listeners
external_addrs:   Vec<Url> // advertised endpoints
source_policy:    optional peer discovery after readiness
visibility/lifetime
```

Create-and-serve validates persistent role and all fields, builds `P2p` with
listeners before start, atomically allocates/reuses the swarm's local-author
reserve partition, runs the initializer, and calls `P2p::start()`. Reserve
exhaustion fails before initializer/listener/author activity. Success requires
listener readiness, not an existing peer, enabling a first server. Peer
discovery may continue afterward. Authoring is registered only after readiness
and still waits for cadence. A newly allocated empty partition is released on
pre-author failure; stopping retains a nonempty partition until its IDs expire.

Promoting joined to serving marks it stopping, fully stops P2p/app state, and
constructs a fresh serving-configured instance with another initializer call.
It never calls inbound reload. Failure leaves no partial server and returns a
typed stopped/recreation error. Namespaced persisted state may be reused.

An externally provisioned onion/I2P endpoint may forward to a distinct local
bind; the API does not conflate them. Built-in transport identity provisioning
is not promised. Locally reused external endpoints produce a linkability
warning.

### D12. Cadence is independent of lifecycle events

One author task uses a fixed version-one 30-minute base interval with
independent uniformly sampled ±10-minute `OsRng` jitter; it is not configurable.
Each emission uses a fresh 32-byte `OsRng` ad ID, configured lifetime capped at
24 hours and defaulting to two hours, and only that swarm's external addresses.
Multi-swarm emission order is shuffled with independent jitter.

Initialization, listener readiness, peer connection, recreation, new overlay
channel, and stop only mutate local author state. They never invoke immediate
send. Stop removes future snapshots; relayed ads expire locally.

### D13. Lilith uses ordinary persistent behavior

Lilith `[overlay]` maps to normal persistent SwarmPool settings and strict policy. It
may be inbound-only or have ordinary outbound peers; it never places overlay
bootstrap into `Settings.seeds`.

Lilith loads the durable store under caps, starts one ordinary overlay, and has
no ad refinery/dialer or local authoring, so its reserve-partition count is zero.
Corrupt records decode fallibly. Malformed/unverifiable seen-ID, quota/reserve,
or epoch state fails startup; address/index state may be quarantined/rebuilt only
when authoritative replay/accounting remains intact. Status RPC reads only
aggregate listener/connection/capacity/address/dedup/eviction/expiry/rejection
counters, including aggregate per-swarm-quota and epoch-checkpoint failures. It
never reports local-author reserve occupancy/use/transition timing and never
walks full IDs/addresses or query mappings.

Legacy instances retain separate settings, paths, registry, policy, and
shutdown.

### D14. Scoped metadata threat model

Protected properties are no stable author identity, no overlay-source-peer to
swarm/authorship persistence, and isolated swarm state.

Disclosed properties are requested ID to responder, connection-level query
linkage, IDs and advertised endpoints observed/mapped by gossip/store peers,
timing/topology evidence, public catalog, endpoint reuse, local full-ID paths
when swarm persistence is enabled, and remote peer retention. The
ID-to-endpoint mapping is intentional rendezvous output. Separate anonymity
circuits may reduce linkage but are not provisioned or guaranteed by SwarmPool.

Absolute cross-swarm unlinkability is rejected because direct query and shared
connections make it false.

## Risks / Trade-offs

- **[Global metadata]** Responders/gossip peers observe IDs → Minimize fields,
  disable public enumeration by default, document disclosure.
- **[Unsigned poisoning]** Fresh forged ads and malicious compatible peers →
  Bound state/work, reservoir-sample every URL in the at-most-16-page terminal
  traversal, CSPRNG-shuffle bounded tiers, partition verified/fresh time and
  attempts, validate compatibility, retain app authorization/static fallback,
  make no authenticity claim.
- **[Dedup saturation]** Protected IDs can fill capacity → Per-swarm general
  quotas prevent one-ID monopolization, local-author partitions preserve
  allocated local cadence, and strict global bounds reject distributed-ID floods
  without early eviction; general remote-ad availability and serving transitions
  can still fail under saturation/churn.
- **[Stale addresses]** No probing retains offline hints → Two-hour receiver and
  author defaults, 24-hour hard maximum, shuffled bounded candidates, deadlines,
  pilot metrics, static fallback.
- **[Client dialing/reflection]** Joiners still try attacker-selected public
  addresses → Resolve once, reject local/reserved ranges, connect the exact
  validated direct socket or configured trusted proxy socket, never locally
  resolve hidden names, reject proxy DNS bypass, enforce per-destination/rate/
  concurrency/total budgets, and retain swarm handshake checks.
- **[Transport abort]** Existing transport constructors/dialers may assume
  validated configuration → Reject unaudited schemes before construction and
  require fallible no-unwind handling across every attacker-selected candidate
  stage and accepted transport adapter.
- **[Clock uncertainty]** Monotonic time does not survive reboot → Restore ad
  address expiry from wall time and replay protection from a periodically saved
  checked monotonic-epoch remainder; crash/downtime may extend records present in
  loaded state, but restart does not reset each to the full horizon.
- **[Storage rollback]** Same-database checkpoints cannot detect rollback before
  an ID commit → Commit seen/address/index state before relay, test/document that
  restoring an older snapshot can permit replay, and make no rollback-resistant
  guarantee.
- **[Cursor churn]** An attacker can mutate indexes between pages → Stateless
  last/terminal-key traversal plus requester key/order/dedup validation guarantees
  bounded forward progress while accepting non-snapshot omissions/additions.
- **[Local reserve metadata]** Reserve records reveal local ephemeral authorship
  to the local database → Store no peer/stable identity and exclude reserve use,
  occupancy, swarm labels, and timing from RPC/status/telemetry.
- **[Role Sybil]** Attackers claim persistent feature → Treat only as hint,
  cache multiple peers, grant no privilege.
- **[Bootstrap concentration]** Configured peers can observe/censor → Multiple
  peers/cache, staged deadlines, static swarm fallback.
- **[Reconstruction cost]** Failed stages rerun P2p/app initialization → Explicit
  bounded attempts and complete cleanup; no unsupported reload semantics.
- **[Serving downtime]** Promotion requires stop/recreate → Require serving mode
  at initial creation where possible and return typed recreation failures.
- **[Transport traces/endpoints]** Transport state or reused endpoints link →
  Separate scope/config, document state, warn reuse, no provisioning claim.

## Deferred Follow-up Changes

These are deliberately not `swarm` completion criteria:

- **Endpoint-reuse enforcement:** A later transport-identity change may reject
  cross-swarm external-endpoint reuse by default and require an explicit
  reduced-privacy override. This change only detects, warns, and documents reuse
  because independent Tor/I2P identity provisioning is unresolved.
- **Query-peer privacy budget:** A later discovery-policy change may specify
  random single-responder lookup and bounded sequential fallback. This change
  retains the current bounded query mechanism and explicitly discloses responder
  and connection-level linkage; it does not promise a selection/fanout policy.
## Migration Plan

1. Land the gated module, descriptor vectors, feature settings, full version size
   validation, bounded messages, and in-memory tests with no app default.
2. Add passive durable storage, protocol work limits, staged fresh-instance
   bootstrap, and lifecycle attempts/recreation using local transports.
3. Add lilith's optional overlay section alongside unchanged legacy sections.
4. Run local multi-node tests for replay saturation, poisoning, no probing,
   per-swarm quota/local reserve, monotonic-epoch restart, two-hour TTL clamp,
   mutation-tolerant terminal cursors, CSPRNG candidate ordering, DNS rebinding/
   local-range/reflection rejection, decoder truncation/hostile lengths, direct/
   proxy exact routing, hidden-service no-local-DNS, full candidate-pipeline no-
   unwind behavior, channel filtering, rollback, first-server creation,
   recreation, and source/query-free stores.
5. Add a default-off application pilot with overlay-then-static policy and
   aggregate privacy-safe metrics.
6. Run required Makefile gates, `@anon-security-review`, and human `src/net`
   review. Broader adoption or legacy deprecation is a later change.

Rollback disables pilot/overlay config and returns to static seeds and legacy
lilith. State is namespaced and removable after shutdown; existing wire and
swarm persistence formats are unchanged.
