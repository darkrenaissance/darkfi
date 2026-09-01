## Why

DarkFi applications currently need independently configured bootstrap peers and
seed lists for every subnet. This makes dynamic subnet discovery, first-server
startup, and shared operational deployment unnecessarily difficult.

Introduce one bounded rendezvous overlay that discovers subnet endpoints while
keeping each subnet in its own ordinary `P2p` instance. Static seeds remain an
explicit fallback and unchanged default during the pilot.

The overlay is a metadata and availability dependency, not an anonymity,
authentication, authorization, or access-control mechanism. It does not remove
per-subnet handshakes, application authorization, endpoint poisoning, Sybil
risk, or availability failure, and it cannot hide a lookup from its responder.

## What Changes

- Add an isolated higher-level `src/net/swarm/` subsystem, exposed as
  `darkfi::net::swarm` behind a dedicated `swarm` feature. `Swarm` owns one
  overlay `P2p` instance at a time and manages independent subnet `P2p`
  instances. Existing BLAKE3, `kvdb-overlay`, serialization, and networking
  facilities are activated explicitly; no third-party dependency is added
  silently.

- Define a versioned, collision-resistant `SubnetId`. Its normative encoding is
  domain-separated, length-delimited, byte-exact, and covered by a golden vector.
  It binds application name, magic bytes, and the major/minor compatibility
  boundary enforced by the current handshake. Patch/prerelease/build metadata do
  not affect the ID. A private descriptor may include a high-entropy 32-byte
  `OsRng` secret, making an unobserved ID difficult to guess without providing
  authentication, encryption, authorization, or confidentiality after
  disclosure.

- Bootstrap through ordinary non-seed overlay sessions, never the existing
  connect-exchange-close `SESSION_SEED` path. A transient first constructs a
  `P2p` from its bounded endpoint-only cache. The cache records only the exact
  connect URL/resolved endpoint pair used by a completed ordinary channel that
  advertised the persistent feature; advertised external addresses are not
  cache authority. On timeout/failure, that instance is fully stopped and a
  fresh configured-peer instance is built. No runtime session reload is assumed.

- Define bounded overlay advertisements, correlated direct lookup, optional
  public enumeration, and bounded errors. Requests use fresh 16-byte IDs echoed
  by responses. Ads contain a fresh 32-byte ad ID, bounded lifetime, visibility,
  and 1..=32 addresses, with no sender timestamp or author identity. Every
  message, URL, page, cursor, pending map, queue, candidate set, and work setting
  has a fixed protocol or safe local maximum.

- Use a stateless terminal-key cursor. Index mutation can add/omit records
  relative to the first page but cannot invalidate the cursor or force restart
  loops. Requesters validate canonical ordering, uniqueness, windows, terminal
  stability, and advancement. Public enumeration is disabled by default and
  requires explicit enablement. Direct lookup ignores visibility; unsigned
  visibility cannot prevent an attacker from relabeling an observed ID public.

- Treat advertisements as untrusted routing hints. Address/replay storage is
  TTL-, cap-, and work-bounded. Replaying a retained global ad ID neither
  refreshes it nor relays it again, including reuse under another subnet.
  Protected IDs are never evicted early.

- Partition replay capacity into a general pool with per-subnet quotas and a
  fixed local-author reserve. Reserve subnet partitions default to 32, max 256,
  and hold exactly 256 IDs each inside the global cap. Remote ads cannot consume
  them. Serving allocates a partition before network activity; stopped subnets
  retain nonempty partitions until expiry. General and reserve state are checked
  separately at startup without double-counting. Reserve occupancy, use,
  failure, and timing remain absent from wire, RPC, status, metrics, and
  telemetry.

- Persist replay deadlines on a monotonic epoch with a 300-second default and
  600-second maximum checkpoint interval. Restart uses checked `u64` arithmetic,
  expires `deadline <= checkpoint`, rejects incoherent deltas, and atomically
  installs a new epoch. Checkpoint failure at 600 seconds fails fresh admission
  and authoring closed while bounded reads may continue.

- Atomically commit seen-ID, quota/reserve, address, and public-index state before
  relay enqueue. Crash/downtime may conservatively extend records present in the
  loaded database. Restoring a database snapshot from before commit can lose an
  ID and permit replay; rollback-resistant storage is not claimed.

- Clamp accepted and locally authored address lifetime to a two-hour default and
  24-hour hard maximum. Relay preserves the validated wire lifetime and each
  receiver applies its own cap. Passive expiry, bounded candidates, deadlines,
  pilot metrics, and static fallback mitigate stale hints without probing.

- Perform no advertisement liveness dialing from overlay stores or Lilith.
  `SubnetId` is one-way, so a store cannot perform the subnet handshake, and a
  transport-only probe would create attacker-controlled scanning/reflection.
  Only a descriptor-holding joining subnet validates returned addresses through
  ordinary magic, application-name, and major/minor compatibility. Compatibility
  and possession of a private ID still grant no application authorization.

- Prevent storage/key order from selecting the resolution/dial prefix. A join
  loads at most 256 bounded previously compatible URLs and reservoir-samples all
  valid URLs returned through terminal completion or the fixed 16-page cap.
  Persisted/fresh tiers and allowed DNS answers are independently shuffled with
  `OsRng`. Candidate preparation time is split between tiers; verified dialing
  ends at a monotonic midpoint and consumes at most half the attempts, preserving
  fresh time/capacity. This reduces ordering bias, not Sybil or authenticity risk.

- Preserve each validated candidate as a typed original-URL/exact-socket target
  through compatibility. It does not enter URL-only host/refinery state first.
  Failed candidates are dropped; a compatible peer may enter bounded ordinary
  retry state. Every later reconnect/refinement resolves, validates, budgets, and
  dials a fresh exact target.

- Apply exact-target egress policy before every untrusted dial. Fresh clearnet
  names resolve once; cached targets revalidate their stored socket without DNS.
  Direct loopback/private/shared/link-local/multicast/unspecified/documentation/
  benchmarking/reserved ranges are rejected outside explicit local-test mode,
  which production Lilith cannot enable. Connection uses the exact socket and
  retains the hostname only for TLS identity.

- For Tor/I2P, the exact socket is the trusted locally configured proxy; an ad
  cannot choose or override it. Canonical hidden-service names are never locally
  resolved and are passed only in matching proxy/TLS handling. Arbitrary
  clearnet remote-proxy DNS is rejected. DNS answer count, resolution/dial rate,
  destination repetition, concurrency, and total candidate work are bounded.

- Standardize the persistent role feature as `("swarm-ad-store", 1)`.
  Persistent nodes retain bounded durable state and relay ads. Transients accept
  no inbound overlay connections, author no ads, and persist no swarm state
  except their bounded successful-endpoint cache. The self-declared feature
  grants no validation, metering, query, or storage privilege.

- Keep overlay and subnet channels structurally separate. Overlay channels remain
  owned by the fixed overlay identity and are never transferred, re-handshaken,
  multiplexed, or reused for subnet data. Subnet `P2p` lifetime remains
  independent.

- Persistent store/gossip and serving-advertisement duties retain the overlay.
  The transient default retains it until explicit stop or Swarm shutdown and
  never disconnects because lookup/join completed. Explicit reduced-privacy mode
  tears down after every caller-visible terminal result—but not an internal join
  lookup—and documents timing correlation. Later discovery reconstructs an
  overlay only when none remains.

- Make metadata disclosure explicit. Ads contain no stable node/signing ID,
  author, relay provenance, or intentional cross-subnet field. Nevertheless,
  responders see requested IDs; requests on one channel are linkable; gossip
  peers observe ID-to-endpoint mappings; timing/topology can suggest origin; and
  endpoint reuse links subnets. The prohibited durable mapping is source peer to
  subnet/authorship, not the rendezvous mapping itself. No PIR or global-observer
  guarantee is made.

- Generate `VersionMessage.node_id` independently with a CSPRNG for every
  overlay/subnet `P2p` instance and process lifetime; do not persist or reuse it
  across networks. Explicit endpoint reuse remains linkable and warning-only.

- Plumb a validated local feature vector into `VersionMessage` without changing
  compatibility or valid wire bytes. Complete outgoing version/verack messages
  are size-checked. Inbound decoding checks every variable length/count before
  reservation/allocation, including strings, semver metadata, addresses, URLs,
  and features. Golden tests preserve existing encoding.

- Audit every swarm, version, verack, and advertised-candidate path as bounded,
  fallible attacker input. URL parsing, DNS, address classification, proxy
  selection/negotiation, transport/TLS dialing, and compatibility contain no
  `unwrap`, `expect`, panic, unchecked indexing/slicing, unvalidated allocation,
  or reachable unimplemented branch. Unsupported/unaudited schemes fail before
  dialer construction.

- Implement registry-owned subnet attempts. Initialization constructs app state,
  protocols, and a shutdown hook before network activity. Join succeeds only on
  a compatible ordinary inbound/outbound/manual channel—not seed, direct, or
  refinement—and failure/timeout/cancellation fully rolls back.

- Support overlay-only, static-only, combined, and overlay-then-static sources.
  Overlay-then-static fully stops the first attempt, constructs a fresh
  static-configured `P2p`, reruns initialization, and remains under one overall
  deadline; it never depends on session reload.

- Select serving before initial start. A first server can initialize and bind
  without an existing peer. Bind and external addresses remain distinct.
  Promoting a joined instance uses serialized full stop/recreate and repeated
  initialization, never inbound reload. Serving is persistent-role only.

- Author ads only after listener readiness on a fixed 30-minute cadence with
  independent uniform ±10-minute `OsRng` jitter. Lifecycle events never trigger
  immediate emission; stop sends no withdrawal. Externally provisioned Tor/I2P
  endpoints are supported without claiming automatic identity provisioning;
  reused endpoints receive an explicit linkability warning.

- Run Lilith as one optional ordinary persistent overlay peer with strict bounds,
  passive durable storage, production egress policy, aggregate-only status, and
  zero local-author reserve partitions. It has no ad refinery/dialer. Legacy
  network sections remain isolated and supported during migration.

- Pilot one application behind a default-off flag with unchanged static fallback.
  Collect bounded aggregate lookup, stale/poisoning, remote store-pressure,
  checkpoint, pagination, and fallback metrics. No metric may contain peer/query/
  private-ID/local-author data.

Deferred follow-up changes, intentionally not implemented here:

- fail-closed cross-subnet endpoint-reuse policy;
- query-peer selection/privacy budgets.

Non-goals: connection multiplexing; authenticated ads; PIR, cover traffic,
global-observer or Sybil resistance; using private IDs as access control; active
ad scanning; automatic hidden-service provisioning; rollback-resistant storage;
or consensus, contract, ZK, canonical serialization, host-ACL, framing, magic,
compatibility, or seed-session changes.

## Capabilities

### New Capabilities

- `swarm-overlay`: versioned IDs; bounded correlated messages/pagination;
  passive replay storage; roles; ordinary-peer bootstrap; exact-target dialing;
  resource limits; and privacy disclosure.
- `subnet-lifecycle`: descriptor resolution; registry-owned app state; ordinary
  join completion; bounded source fallback; isolated state; serving/recreation;
  and teardown.
- `lilith-overlay-seed`: optional persistent ordinary overlay peer with bounded
  passive durable state, aggregate status, strict privacy/resource policy, and
  legacy isolation.

### Modified Capabilities

None. No existing specifications under `openspec/specs/` are modified.

## Impact

- **New code:** `src/net/swarm/`, gated by feature `swarm`.
- **Existing `src/net`:** module exposure; validated version-feature plumbing;
  bounded version/verack decoding and outgoing size checks; exact validated
  target dialing and bounded one-/two-phase pre-start plans; narrow channel/host
  helpers; and fallible transport adapters or pre-construction rejection.
  Existing valid wire bytes, framing, magic, compatibility, and manual/seed/
  inbound reload semantics remain unchanged.
- **Feature/dependencies:** existing optional facilities are activated explicitly.
  Any new dependency/source, `build.rs`, or proc-macro requires separate human
  supply-chain review.
- **Lilith:** one optional overlay section and aggregate status; legacy sections
  remain available.
- **Pilot:** opt-in Swarm construction and descriptor pinning with static fallback
  unchanged by default.
- **Operational cost:** one additional bounded overlay connection set/store plus
  independent per-subnet listeners.
- **Security review:** this is a privacy-sensitive shared-network change.
  Transport/privacy, persistence, descriptor hashing, and candidate fairness
  require focused human review. `@anon-security-review`, CI, and final human
  patch review remain mandatory.
