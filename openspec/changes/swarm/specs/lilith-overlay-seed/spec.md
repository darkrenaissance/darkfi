## Purpose

Defines lilith as a persistent ordinary overlay peer providing bounded swarm
rendezvous through one listener and durable store without joining, probing, or
serving advertised swarms.

## ADDED Requirements

### Requirement: Single ordinary overlay configuration

Lilith SHALL support one overlay section containing accept and external
addresses, ordinary bootstrap peers, connection policy, datastore/hostlist/ad
store paths, public-enumeration policy, and finite resource limits within
`swarm-overlay` maxima. It SHALL start one ordinary
overlay `P2p` advertising `("swarm-ad-store", 1)`.

Bootstrap addresses MUST be ordinary peers, not seed sessions. Lilith MAY run
inbound-only with zero ordinary outbound slots as an operator topology choice.
Production lilith overlay configuration MUST disable local-test egress mode;
every configured outbound target SHALL use the exact resolved-target validation
and dial budgets from `swarm-overlay`.
It SHALL NOT require a per-swarm listener, descriptor, magic, datastore, or
network instance to store ads and answer lookup. Learning an ID MUST NOT make
lilith join or serve it.

#### Scenario: Overlay-only deployment

- **WHEN** a valid overlay section exists without legacy sections
- **THEN** lilith starts one persistent ordinary overlay peer

#### Scenario: Inbound-only topology

- **WHEN** an overlay listener has zero outbound slots
- **THEN** inbound ordinary peers can maintain sessions, submit ads, and query

#### Scenario: Invalid configuration

- **WHEN** any path, address, privacy policy, or resource limit is invalid
- **THEN** startup fails before overlay activity without panic or unbounded
  fallback

### Requirement: Unknown swarms need no reconfiguration

Lilith SHALL validate, store, relay, and answer valid ads for previously unknown
IDs within all message/store/work bounds and without descriptors. Learning new
IDs MUST NOT create swarm listeners or app protocols.

#### Scenario: Fresh swarm ad

- **WHEN** a valid unknown-ID ad arrives
- **THEN** it becomes available to bounded lookup without restart or operator
  action

#### Scenario: Rendezvous-only learning

- **WHEN** many swarm IDs are learned
- **THEN** lilith still runs one overlay and no swarm instance

### Requirement: Durable state preserves bounded replay and expiry

Lilith SHALL persist normalized per-swarm address records with their current
visibility/expiry, local expiry metadata, stateless ordered indexes, protected
replay IDs, and monotonic-epoch checkpoint metadata. It SHALL enforce configured
caps no greater than 1,024 addresses per swarm, 65,536 total addresses, 1,024
general-pool protected IDs per swarm, and 262,144 protected IDs globally.
Accepted address lifetime SHALL be clamped to the configured local receive cap,
defaulting to 7,200 seconds and never exceeding 86,400 seconds. Restart MUST
restore only remaining ad-address lifetime and MUST NOT revive expired entries.

At least every 600 seconds and on clean shutdown, lilith SHALL atomically persist
the current monotonic epoch checkpoint, defaulting to 300 seconds. Restart SHALL
compare unsigned 64-bit deadline/checkpoint ticks before checked subtraction:
expired/equal records are removed; valid deltas over 173,400 seconds, arithmetic
failure, or failed checked `Instant` addition are typed startup errors; remaining
duration is capped at 172,800 seconds. Surviving records and the new epoch SHALL
be replaced atomically. It MUST NOT reset every record present in the loaded
database to a fresh horizon or shorten it. Crash/downtime MAY extend that
remainder. Rollback before an ID's commit can remove replay state and permit
replay; lilith makes no non-rollbackable guarantee.

Checkpoint failure reaching the 600-second maximum SHALL make lilith reject
fresh ads until checkpoint recovery or controlled overlay shutdown; existing
bounded lookup MAY continue.

Protected IDs MUST NOT be evicted early. Lilith authors no swarm ads and SHALL
configure zero local-author reserve partitions. If a swarm quota or its global
general pool has no expired slot, the applicable fresh remote ad SHALL be
rejected rather than weakening replay protection.
Persistence MUST NOT contain ad sources, queriers, query history, source-peer/
swarm associations, or private secrets. Replay ID-to-advertised-swarm binding
solely for quota accounting is allowed and MUST NOT contain a peer/source.
Decoding malformed/truncated records SHALL be fallible and bounded. Malformed or
unverifiable seen-ID, quota/reserve, or epoch state SHALL fail overlay startup.
Address records MAY be quarantined and the public index rebuilt only when replay
and accounting state remains intact. Capacity failure SHALL not evict/reset
protected state.

Seen-ID/quota state, address/index mutation, and acceptance SHALL commit
atomically before relay enqueue. Commit failure performs neither mutation nor
relay.

#### Scenario: Restart preserves remaining lifetime

- **WHEN** lilith restarts before expiry without wall-clock rollback
- **THEN** only remaining lifetime is restored

#### Scenario: Restart does not revive expiry

- **WHEN** restart occurs after expiry
- **THEN** the ad is not returned

#### Scenario: Protected set is full

- **WHEN** all dedup slots are protected and a fresh ad arrives
- **THEN** lilith rejects it without evicting protected replay state

#### Scenario: Restart restores seen-ID remainder

- **WHEN** lilith loads valid persisted seen IDs after any clock movement
- **THEN** each receives its conservative checkpointed remainder on a new
  monotonic epoch rather than a fresh full horizon

#### Scenario: One swarm reaches its replay quota

- **WHEN** one claimed swarm consumes all of its unexpired general-pool slots
- **THEN** lilith rejects another fresh ad for it without consuming other
  swarm capacity

#### Scenario: Store rollback loses accepted ID

- **WHEN** lilith loads a database snapshot from before an ad's atomic commit
- **THEN** replay may be accepted again and no rollback-resistant claim is made

#### Scenario: Reduced dedup cap blocks startup

- **WHEN** configured capacity cannot hold valid persisted seen IDs
- **THEN** overlay startup fails without evicting them

#### Scenario: Corrupt record

- **WHEN** durable bytes are malformed or truncated
- **THEN** authoritative replay/accounting corruption fails startup, while only
  non-authoritative address/index state may be quarantined/rebuilt fallibly

### Requirement: Lilith performs no advertisement liveness dialing

Lilith MUST NOT connect to an advertised swarm address due to accepting,
storing, relaying, expiring, or reporting an ad. It SHALL expire through local
TTL, replay admission, and capacity policy only. It MUST NOT describe transport
reachability as swarm compatibility; only a descriptor-holding joining app can
perform the swarm handshake.

#### Scenario: Attacker-selected address

- **WHEN** an accepted ad contains an attacker-selected shareable address
- **THEN** no lilith ad-store task connects to it

#### Scenario: Passive expiry

- **WHEN** an ad expires
- **THEN** it stops being returned without a probe

### Requirement: Cold-start lookup uses an ordinary correlated session

A client SHALL be able to configure lilith as an ordinary peer, establish a
long-lived session, and issue request-ID-correlated bounded lookups. Lilith MAY
return locally unexpired addresses whose servers are offline; responses are
untrusted hints, not reachability proof.

#### Scenario: Fresh client queries directly

- **WHEN** a no-cache client connects to lilith as an ordinary peer
- **THEN** it can query without a seed-session exchange

#### Scenario: Stored address is stale

- **WHEN** a returned unexpired address is offline
- **THEN** the joining app handles failure within its deadline and lilith makes
  no availability guarantee

### Requirement: Strict bounded resource policy

Lilith's overlay SHALL enforce all protocol message, URL, page, pending request,
store, dedup, work, relay-fanout, and configured-safe maxima with strict ban
policy. Small requests MUST NOT induce unbounded response, cursor, write,
relay, allocation, or connection work. Legacy policy MUST NOT weaken overlay
policy.

#### Scenario: Query flood

- **WHEN** one channel exceeds message or work budgets
- **THEN** strict penalties apply while state remains bounded

#### Scenario: Ad flood

- **WHEN** fresh ads reach address or protected-ID caps
- **THEN** deterministic rejection/eviction rules preserve every bound and
  replay guarantee

#### Scenario: Legacy relaxed policy

- **WHEN** a legacy instance is relaxed
- **THEN** overlay strict policy remains independent

### Requirement: Aggregate-only overlay status

Status RPC SHALL expose listener state, aggregate connection counts, configured
capacities, current address/dedup counts, evictions, rejections, and expiries.
It MUST NOT expose peer or advertised addresses, queried/private swarm IDs,
ad sources, per-peer counters, query history, or source/query associations.
Public enumeration, if enabled, remains the bounded overlay protocol.

#### Scenario: Operator reads health

- **WHEN** status is requested
- **THEN** aggregate health/capacity/count metrics are returned without peer or
  swarm-query identifiers

#### Scenario: Querier data is absent

- **WHEN** peers query different IDs
- **THEN** status and durable metrics cannot identify which peer queried which
  ID

### Requirement: Legacy sections remain isolated during migration

Lilith SHALL continue accepting valid legacy sections as independent `P2p`
instances. Overlay and legacy settings, listeners, paths, protocol registries,
policies, failures, and shutdown handles MUST remain isolated. Legacy refusal
requires a later release-boundary plan.

#### Scenario: Mixed configuration

- **WHEN** overlay and legacy sections coexist
- **THEN** each runs with independent state and policy

#### Scenario: Overlay failure

- **WHEN** overlay startup or runtime fails
- **THEN** failure is reported without silently changing legacy configuration

#### Scenario: Legacy-only deployment

- **WHEN** currently valid legacy sections exist without overlay
- **THEN** they remain accepted during migration
