# Proposal: swarm overlay for subnet discovery

## Why

Every DarkFi P2P network (darkirc, taud, darkfid testnet, fud, ...) is an
isolated `P2p` instance separated by magic bytes and `app_name`. As a
consequence:

- Each app ships and maintains its own per-network seed lists.
- Lilith must run one listener, datastore, hostlist, and config section per
  network, and must be reconfigured and restarted to serve a new network.
- There is no way to discover a subnet dynamically: creating a private
  darkirc room, a testnet fork, or a fud channel with its own membership
  requires baking new magic bytes into configs on every participating node.
- Nodes that serve multiple networks expose one inbound endpoint per network,
  and the seed infrastructure becomes the de-facto directory of all networks.

We introduce a thin gossip overlay ("swarm") for subnet rendezvous: a single
overlay network through which nodes learn which subnets exist and which peers
serve them, then join each subnet directly as an ordinary `P2p` network. One
overlay seed list serves all networks forever; lilith is demoted from an
N-network zookeeper to a single overlay seed with a persistent ad store.

Privacy is a primary driver, not an afterthought: the overlay must not become
a cross-subnet correlation point (hard invariant: no peer-address/metadata
leakage). Ads are per-subnet, carry per-subnet addresses, and propagate by
gossip so origins stay ambiguous, mirroring the properties of the existing
address protocol.

## What Changes

- Add a new `src/swarm` subsystem: a `Swarm` that owns one overlay `P2p`
  instance (fixed `app_name`, one magic-bytes constant) and dynamically
  manages subnet `P2p` instances.
- Define `SubnetId = blake3(canonical descriptor)`, where the descriptor
  binds `{app_name, magic_bytes, version, optional secret}`. Known subnets
  are pinned in app code; the optional secret yields unguessable private
  subnets (obscurity-based access control).
- New overlay wire messages (`SubnetAd`, `GetSubnets`/`Subnets`,
  `GetSubnetAddrs`/`SubnetAddrs`) defined via the existing
  `impl_p2p_message!` macro with metering configurations, gossiped by a new
  `ProtocolSwarm` registered through the existing `ProtocolRegistry`.
- Ads are unsigned, `{subnet_id, addrs, ttl}`, propagated by gossip/flood
  (like `AddrsMessage`); poisoning is handled by the existing refinement and
  ban machinery, not by signatures. No stable node key crosses subnets.
- Subnet membership discovered on the overlay is seeded into each subnet's
  greylist; per-subnet hostlists, refinement, and datastores remain unchanged
  and self-maintaining.
- Serving a subnet (advertising inbound addrs, e.g. one onion per subnet) is
  opt-in per subnet; joining (query + dial) is the default.
- Overlay node roles: **persistent** swarm nodes (desktop daemons, lilith)
  keep disk-backed ad stores and relay gossip; **transient** swarm nodes
  (e.g. mobile apps doing a lookup) run inbound-free overlay sessions,
  emit no ads, and persist at most an overlay hostlist cache so repeat
  sessions dial cached peers instead of always contacting seeds. The role
  is declared via the existing `VersionMessage` features vector — no new
  handshake semantics.
- Lilith is reduced to a single overlay listener plus a persistent,
  TTL-bounded ad store for cold-start; its config collapses from N network
  sections to one overlay section and new subnets are learned without
  operator action.
- Minimal, non-breaking changes to `src/net` (exports/helpers only); no
  changes to channel framing, magic-byte gating, version handshake
  semantics, or the host ACL.

Non-goals: multiplexing subnets over a single connection (rejected for
traffic-analysis and architectural reasons); replacing gossip with structured
DHT lookup for membership; subnet-scoped content DHT keys (possible later
rider on the same overlay); changes to consensus, contracts, or ZK.

## Capabilities

### New Capabilities

- `swarm-overlay`: the overlay rendezvous protocol — SubnetId derivation,
  advertisement format, gossip propagation, query handling, ad-store TTL and
  bounds, metering, node roles (persistent vs transient and their
  obligations), and the anti-linkability requirements (per-subnet addresses,
  origin ambiguity, no cross-subnet identity).
- `subnet-lifecycle`: dynamic subnet management from the application
  perspective — resolving a SubnetId to reachable peers, spawning/stopping
  subnet `P2p` instances at runtime, seeding subnet greylists from overlay
  ads, and the serving-vs-joining modes with their advertisement obligations.
- `lilith-overlay-seed`: lilith redeployed as a single overlay seed — one
  listener, persistent ad store with refinery-based expiry, single-section
  config, and dynamic learning of new subnets.

### Modified Capabilities

None. There are no existing specs under `openspec/specs/` to modify; `src/net`
core behavior is deliberately left unchanged.

## Impact

- **New code**: `src/swarm/` (overlay `P2p` ownership, `ProtocolSwarm`, ad
  store, subnet registry/lifecycle) and its wire message definitions.
- **`src/net`**: minor only — possibly exporting small `hosts` helpers and a
  path-derivation helper for per-subnet datastores; no semantic changes to
  sessions, hosts, channel, or transport layers.
- **`bin/lilith`**: config format collapses to one overlay section; existing
  per-network sections deprecated on a migration path.
- **Apps** (`bin/darkirc`, `bin/tau`, `bin/darkfid`, `bin/fud`): opt-in
  adoption — construct `Swarm` instead of (or in front of) individual `P2p`
  instances; static seed lists remain as fallback during transition.
- **Security review focus**: the overlay is a new metadata surface; the
  anti-linkability requirements in `swarm-overlay` are binding and must pass
  the anon-security-review gate before apply.
