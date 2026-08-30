# Design: swarm overlay for subnet discovery

## Context

Today a "network" is one `P2p` instance identified by `{magic_bytes,
app_name, app_version}`. Magic bytes are checked at channel setup
(`src/net/channel.rs`, raw frame read) before the version/verack handshake
checks `app_name`. Hostlists, refinement, and datastores are per-`P2p`.
Lilith spawns one `P2p` per configured network (`bin/lilith/src/main.rs`,
`spawn_net()`), each needing its own listener, datastore, and config section.
Apps construct their own `P2p` at startup from static seed lists.

Extension points already public in `src/net` and sufficient for an overlay
without core surgery:

- `ProtocolRegistry::register(session_flags, constructor)` — attach custom
  protocols per session type (`protocol_registry.rs`).
- `#[macro_export] impl_p2p_message!` (`message.rs`) — define new wire
  messages with metering; already used by `event_graph` and fud.
- `GetAddrsMessage`/`AddrsMessage` gossip via `ProtocolAddress` establishes
  the pattern ads should follow: relayed, unsigned, refinement-filtered.
- `src/dht` exists for content-keyed lookup (Kademlia) and is used by fud;
  it is the wrong tool for membership (structured lookup paths are linkable).

See proposal.md for motivation. Constraints that shape this design: no
changes to channel framing, magic-byte gating, or handshake semantics; the
overlay must not become a cross-subnet correlation point; no stable node
identity may cross subnets.

## Goals / Non-Goals

**Goals:**

- One overlay seed list bootstraps discovery for all subnets, forever.
- Subnets remain ordinary `P2p` networks: their own magic bytes, hostlist,
  refinement, datastore — joined by direct dial after discovery.
- Subnet spawn/stop at runtime, driven by `SubnetId`.
- Anti-linkability as a designed property, not a config flag.
- Lilith collapses to one listener + persistent ad store.
- First-class transient (mobile) participants: cheap lookups, no relay
  obligations, no on-disk state beyond an optional overlay hostlist cache,
  invisible to third parties.

**Non-Goals (design-level):**

- No connection multiplexing (one connection carrying multiple subnets).
- No DHT lookup for membership; gossip only.
- No signatures on ads; authenticity is refinement's job.
- No changes to `src/net` semantics — additive exports only.
- No automatic serving of every joined subnet; serving is explicit per
  subnet.

## Decisions

### D1. Thin overlay, not multiplexing or subnet-tagged address protocol

Three alternatives were considered:

- **Multiplexed overlay** (one connection, virtual streams per subnet):
  breaks the 1:1 channel↔network invariant across `session/`+`channel.rs`,
  and mixes subnet traffic on one wire — a traffic-analysis surface that
  violates the anonymity constraints.
- **Subnet-tagged `GetAddrs`/`Addrs`** (one global network carrying all
  subnets): smallest diff, but merges all hostlists into one refinement
  state, exposing cross-subnet membership in a node's address book and
  connection churn.
- **Thin overlay (chosen)**: `Swarm` owns one overlay `P2p` plus a
  `HashMap<SubnetId, SubnetEntry>`; each `SubnetEntry` owns an ordinary
  spawned `P2p`. Overlay only bootstraps; subnet health remains
  self-maintaining via existing refinery.

### D2. `SubnetId` = `blake3(canonical descriptor)`

```
descriptor := app_name || magic_bytes || version_constraint || secret?
SubnetId   := blake3(descriptor)
```

- Apps pin known IDs (e.g. darkirc mainnet) — a pin is a spec of the
  descriptor fields.
- `secret` present → unguessable ID: a non-member cannot even name the
  subnet, giving obscurity-based access control (rendezvous-string style).
- `version_constraint` is part of the descriptor so a subnet's version
  policy is fixed at creation; exact pin initially, ranges deferred.

Alternative: human-readable subnet names — rejected: global names leak the
set of private subnets into gossip and invite squatting.

### D3. Ad format and propagation: unsigned gossip with TTL

```
SubnetAd { subnet_id, addrs: Vec<(Url, u64)>, ttl_secs }
```

- Propagated by flood/gossip identical in spirit to `AddrsMessage`; the peer
  an ad is received from is not its author → origin ambiguity.
- Unsigned, no node identity. Poisoning is bounded by (a) refinement — ads
  land in the target subnet's greylist and dead addrs are dropped by
  handshake checks, and (b) per-message metering + ban policy for floods.
- Overlay ad stores keep entries until `ttl` expiry + refinery liveness
  checks (lilith's existing whitelist-refinery pattern, retargeted at ads).
- Ads are re-gossiped on a slow, jittered cadence (like refinery intervals),
  never event-triggered on subnet start — see R3.

Alternative: per-subnet signing keys. Gives poisoning resistance but tempts
key reuse across subnets (linkability) and adds key management; deferred
until refinement proves insufficient.

### D4. New messages and `ProtocolSwarm`, all outside `src/net` core

```
SubnetAd                       (gossip, unsolicited)
GetSubnets       → Subnets             (list known subnet_ids)
GetSubnetAddrs{subnet_id} → SubnetAddrs{subnet_id, addrs}
```

Defined via `impl_p2p_message!` with metering configurations and
`MAX_BYTES` estimates in the existing style. `ProtocolSwarm` is registered
via `ProtocolRegistry` on outbound+inbound sessions of the overlay `P2p`
only. `GetSubnets` responses are built from local ad-store state; queries
reveal participation in the overlay but not in any particular subnet.

### D5. Serving vs joining

- **Joining** (default): pull `SubnetAddrs`, seed the subnet `P2p`'s greylist
  (via `Hosts::insert`, grey), dial. No ad is emitted.
- **Serving** (opt-in per subnet): requires inbound addrs for that subnet —
  one tor/i2p onion per subnet is the recommended deployment so overlay
  observers cannot correlate a shared endpoint across subnets. Emits ads on
  the D3 cadence.

### D6. Subnet lifecycle under `Swarm`

- `Swarm::join(subnet_descriptor)` → resolve via overlay → spawn subnet
  `P2p` with per-subnet `p2p_datastore`/`hostlist` paths derived from
  `SubnetId` under a swarm-managed directory; register app protocols onto
  that `P2p`'s registry before `start()`.
- `Swarm::serve(subnet_descriptor, inbound_cfg)` → join + advertise.
- `Swarm::leave(subnet_id)` → `P2p::stop()` + deregister; ads simply expire
  via TTL (no "leave" message — a departure broadcast would create a
  timing-correlation surface).
- Dynamic spawn after startup is the main new runtime pattern; apps
  currently build all networks before `start()`. Watch item: executor
  shutdown ordering when many subnet `P2p`s stop concurrently.

### D7. Lilith becomes an overlay seed

One config section (`[overlay]`: accept addrs, datastore), no per-network
sections. Runs the overlay `P2p` with `inbound_connections` high,
`outbound_connections` 0 (unchanged posture: no outbound dialing), plus:
persistent ad store (ads survive restarts until TTL), refinery-based ad
expiry, and the `spawns` RPC retargeted at overlay stats (known subnets,
ad counts). Existing per-network sections keep working during migration
(lilith simply spawns those nets as before, alongside the overlay).

### D8. Overlay node roles: persistent vs transient, declared not negotiated

Desktop daemons and lilith run on always-on machines and carry the overlay;
mobile apps (and any short-lived client) join the overlay only to look
subnets up and leave. The distinction is declared through the existing
`VersionMessage.features` vector (`src/net/message.rs`), which is on the
wire today but sent empty: a persistent node advertises
`("swarm-store", 1)`; a transient node sends no swarm feature. Role is
self-declared and unauthenticated — it is a hint for policy and load, never
a privilege.

|                      | persistent                     | transient                        |
|----------------------|--------------------------------|----------------------------------|
| typical host         | desktop daemon, lilith         | mobile app doing a lookup        |
| inbound addrs        | typical (often onion)          | none (`inbound_connections: 0`)  |
| ad store             | disk-backed, TTL + refinery    | none (optional in-memory cache)  |
| gossip relay         | yes                            | only while connected (brief)     |
| subnet serving       | per-subnet opt-in (D5)         | never                            |
| overlay outbound     | default slots                  | minimal (1–2), query then leave  |
| datastore/hostlist   | persisted                      | overlay hostlist cache (TSV); no ad store |
| heartbeat tuning     | default                        | longer intervals (battery, NAT)  |

- **Uniform protocol behavior**: both roles answer `GetSubnets`/
  `GetSubnetAddrs` from whatever local state exists while connected. Role
  changes *what state exists*, never message handling — role-specific wire
  behavior would fingerprint peers and split the anonymity set.
- **Transients are invisible to third parties**: no inbound addrs and no ads
  means a transient node never appears in any hostlist or ad store. Its
  overlay peers see only a short-lived connection — the same exposure a
  client of today's per-network seeds has.
- **Transient hostlist cache**: a transient node persists its overlay
  hostlist (peer addresses only, via the existing `net::Settings.hostlist`
  TSV — zero new machinery) so later sessions dial cached overlay peers
  first and fall back to configured seeds only on miss/failure. The cache
  MUST NOT record queried or joined subnets — it contains overlay peer
  addresses and nothing else. Local-device forensics trade-off: the cache
  proves overlay participation but not subnet membership; privacy-maximal
  deployments disable it (also weaker against stale-entry churn, handled by
  normal greylist refinement).
- **Lilith is just the canonical persistent node**; any persistent daemon
  relays ads and can cold-start others, which strengthens the R4 mitigation.
- **Load spreading without new messages**: persistent nodes are reachable
  addrs in the overlay's own hostlist (they advertise inbound via the normal
  address protocol), so a transient node that wants to avoid hammering seeds
  dials overlay peers from `GetAddrs` and simply keeps the ones whose
  handshake carries the `swarm-store` feature. A dedicated
  feature-filtered query can be added later inside swarm's message set if
  wasted dials prove costly; not needed initially.
- Mobile constraints shape defaults, not the protocol: battery (longer
  heartbeat via `NetworkProfile`, disconnect after lookup), NAT (no inbound,
  no hole punching required), metered data (small `GetSubnetAddrs` replies
  bounded by metering).

Alternative considered: no declared role at all (purely emergent — transients
are just nodes that leave quickly). Rejected: without the feature bit,
persistent nodes cannot preferentially keep slots for ad-carrying peers, and
transients cannot find store-keeping peers without trial dialing everyone.

## API Sketch

Illustrative signatures — names may shift during implementation; the shape
is what apps program against. Everything mirrors the existing `P2p` idiom:
async constructors returning `Result`, `Arc` pointers, `StoppableTask`
lifecycle, `net::Settings` for transport-level config.

### Core types

```rust
/// Declared overlay role (D8)
pub enum SwarmRole {
    /// Disk-backed ad store, gossip relay, may serve subnets
    Persistent { datastore: PathBuf },
    /// Lookup client; no ads, no inbound, optional overlay hostlist cache
    /// (`None` leaves no on-device overlay trace)
    Transient { hostlist: Option<PathBuf> },
}

/// Canonical subnet descriptor (D2)
pub struct SubnetDescriptor { /* app_name, magic_bytes, version, secret? */ }

impl SubnetDescriptor {
    /// Pin a released network; shipped as constants in app code
    pub const fn pinned(app_name: &str, magic_bytes: [u8; 4], version: &'static str) -> Self;

    /// Secret-bearing descriptor for private subnets
    pub fn private(app_name: &str, magic_bytes: [u8; 4], version: &str, secret: &[u8]) -> Self;

    /// BLAKE3 of the canonical serialization
    pub fn id(&self) -> SubnetId;
}

/// Handle to a joined or served subnet
pub struct SubnetHandle { /* ... */ }

impl SubnetHandle {
    pub fn id(&self) -> SubnetId;
    /// The subnet's own P2p instance, for app-level messaging
    pub fn p2p(&self) -> P2pPtr;
}

pub struct Swarm { /* overlay P2p + subnet registry + ad store */ }
pub type SwarmPtr = Arc<Swarm>;
```

### Usage: persistent daemon (desktop, e.g. darkirc)

```rust
// Overlay settings: one seed list, forever
let overlay = net::Settings {
    app_name: "swarm".into(),
    magic_bytes: OVERLAY_MAGIC,
    seeds: OVERLAY_SEEDS.into(),
    inbound_connections: 64,
    ..Default::default()
};

let role = SwarmRole::Persistent {
    datastore: "~/.local/share/darkirc/swarm/ads".into(),
};
let swarm = Swarm::new(role, overlay, ex.clone()).await?;
swarm.clone().start().await?;

// Pinned descriptor shipped with the app (D2)
const DARKIRC: SubnetDescriptor =
    SubnetDescriptor::pinned("darkirc", [251, 229, 199, 181], "0.5.1");

// Client-only participation (default)
let subnet = swarm.join(&DARKIRC, darkirc_protocols).await?;

// Or serve it, with this subnet's own onion (D5)
let inbound = vec![Url::parse("tor://darkirc-7.onion:9440")?];
let subnet = swarm.serve(&DARKIRC, inbound, darkirc_protocols).await?;

// App messaging rides the subnet's ordinary P2p — unchanged app code
let _ = subnet.p2p();

// Later: silent leave (D6) — no departure message, ads expire by TTL
swarm.leave(DARKIRC.id()).await?;
```

`darkirc_protocols` is the registration closure the swarm runs against the
subnet `P2p`'s protocol registry *before* `start()` — the same
`registry.register(session_flags, init)` hook `register_default_protocols`
uses internally. Apps keep registering their protocols exactly as today;
they just do it through the closure.

### Usage: transient lookup (mobile)

```rust
let overlay = net::Settings {
    app_name: "swarm".into(),
    magic_bytes: OVERLAY_MAGIC,
    seeds: OVERLAY_SEEDS.into(),
    outbound_connections: 2,
    inbound_connections: 0,
    ..Default::default()
};

let role = SwarmRole::Transient {
    // Cache overlay peers between sessions so later sessions dial cached
    // peers first and only fall back to seeds. `None` for a device free
    // of overlay traces.
    hostlist: Some(cache_dir.join("overlay_hostlist.tsv")),
};
let swarm = Swarm::new(role, overlay, ex.clone()).await?;

// Lookup only: resolve addresses, no subnet participation, then disconnect
let addrs: Vec<Url> = swarm.lookup(&FUD_CHANNEL).await?;

// Or join for the duration of the app session (recommended over
// per-lookup connections — R7 battery-vs-mixing guidance)
let subnet = swarm.join(&FUD_CHANNEL, fud_protocols).await?;
```

### Usage: lilith (the canonical persistent node)

```rust
let role = SwarmRole::Persistent { datastore: cfg.adstore };
let swarm = Swarm::new(role, overlay_settings, ex).await?;
swarm.clone().start().await?;
// Nothing else. No per-subnet config, no join calls: ads arrive by
// gossip, and the durable store + refinery (D7) make lilith the
// cold-start anchor. Legacy per-network sections still spawn ordinary
// P2p instances alongside, during migration.
```

### API-enforced invariants

- `serve()` on a `Transient` swarm fails fast — the transient role has no
  serving path (spec: swarm-overlay, node roles).
- `join`/`serve` take a descriptor, never a raw id: a caller cannot join a
  subnet it cannot describe, and the spawned `P2p` still enforces the
  subnet's own magic bytes and `app_name` handshake independently of the
  overlay.
- Protocol registration happens only before subnet `start()` via the
  closure — there is no window where a subnet accepts connections without
  its app protocols attached.
- `lookup()` answers from local state first (on a persistent node: the ad
  store) and queries the overlay only on miss; on a transient node it may
  reuse the session cache.
- A transient swarm dials its cached overlay hostlist before configured
  seeds; seeds are only the first-ever-run and fallback path.

## Risks / Trade-offs

- **R1 Overlay as correlation point** (new metadata surface) → per-subnet
  addresses (D5), unsigned per-subnet ads with no node identity (D3),
  gossip origin ambiguity (D3/D4), query design that reveals only overlay
  participation (D4). Residual: a global adversary observing all overlay
  traffic plus all subnet on/off timings can still correlate — documented
  as a known limit; timing jitter is the only partial defense.
- **R2 Ad poisoning / flood** → no signatures means ads are cheap to forge;
  bounded by refinement liveness checks, metering thresholds, and ban
  policy; per-subnet ad-store caps (like GREYLIST_MAX_LEN) bound memory.
- **R3 Timing linkage of fresh serving nodes** → ads on jittered cadence
  only, never event-driven; deployment guidance recommends pre-registered
  onions.
- **R4 Cold-start still depends on overlay seeds** → same trust profile as
  today's per-network seeds, but strictly reduced: the seed sees overlay
  participation only, never which subnets are joined (dials are direct and
  subnet-scoped). Multiple overlay seeds can be listed, any serving node's
  overlay connection also relays ads, and repeat sessions bootstrap from
  the transient hostlist cache rather than seeds.
- **R5 Private-subnet obscurity is not access control** → the secret names
  the subnet; it does not encrypt subnet traffic. Documented; end-to-end
  protections remain the apps' job (e.g. darkirc saltbox, event-graph RLN).
- **R6 Tor/onion-per-subnet operational cost** → serving on clearnet tcp is
  possible but exposes a shared endpoint; the design allows it, deployment
  guidance should not recommend it.
- **R7 Transient nodes are timing-fingerprintable** (connect → query →
  leave) → while connected, transients send the same messages any node may
  send, and third parties never observe them at all (no hostlist presence).
  The connection-lifetime pattern itself is the residual fingerprint; cover
  traffic is out of scope for battery-constrained devices. Documented
  battery-vs-mixing tension: staying connected longer mixes better and costs
  more — deployment guidance may suggest holding the overlay connection for
  the app session rather than per lookup.
- **R8 Transient load concentrates on overlay seeds** → transients spread
  across persistent nodes via the overlay's own address gossip plus the
  `swarm-store` handshake feature (D8, no new messages); after the first
  session, the transient hostlist cache (D8) means seeds are fallback-only.
  If concentration persists, add a feature-filtered query inside swarm's
  message set.

## Migration Plan

1. Land `src/swarm` with the overlay protocol; lilith gains the overlay
   section alongside legacy per-network sections. No app changes.
2. Apps adopt `Swarm` optionally, keeping static seed lists as fallback;
   pinned `SubnetId` constants added per app (values equal to existing
   `{app_name, magic_bytes, version}` triples so current networks are
   discoverable).
3. Once overlay coverage is healthy, deprecate lilith's per-network
   sections (warn, then refuse across a release boundary).
4. Rollback: overlay is additive; apps revert to static seeds, lilith drops
   the overlay section. Ads and `SubnetId` paths are all namespaced and
   removable.

## Open Questions

- Exact `ttl_secs` default and per-subnet ad-store cap values (tune during
  implementation against refinery intervals).
- Whether `GetSubnets` responses should be rate-limited per peer beyond
  generic metering (decide when metering thresholds are set).
- Whether darkfid's consensus networks adopt `Swarm` at all, or only
  latency-tolerant apps do (darkirc, taud, fud) — an adoption-policy
  question, not a protocol one.
- Whether persistent nodes should cap the share of inbound slots given to
  transient (feature-less) overlay peers, and at what ratio — deployment
  tuning once real transient traffic exists.
