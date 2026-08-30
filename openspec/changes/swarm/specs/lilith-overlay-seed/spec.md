## Purpose

Defines lilith redeployed as a single persistent overlay seed: one overlay
listener and datastore, a durable advertisement store with refinery-based
expiry, dynamic learning of new subnets without operator action, and
continued support for legacy per-network sections during migration.

## ADDED Requirements

### Requirement: Single overlay seed configuration

Lilith SHALL support an overlay configuration section that starts one
overlay network instance with its own accept addresses, datastore, and
hostlist paths. When the overlay section is present, lilith participates in
the overlay as a persistent node: high inbound slot allowance, no outbound
slot requirement, gossip relay, and the swarm-ad-store handshake feature.
Per-network configuration sections SHALL NOT be required for the overlay to
operate, and operating the overlay SHALL NOT require one listener, datastore
section, or magic-bytes constant per served subnet.

#### Scenario: Fresh subnet served without reconfiguration

- **WHEN** participants begin advertising a subnet unknown to a running
  lilith overlay seed
- **THEN** lilith's advertisement store learns and serves the subnet with
  no operator action and no restart

#### Scenario: Overlay-only deployment

- **WHEN** lilith is configured with only the overlay section
- **THEN** it starts, accepts overlay connections, and serves subnet
  discovery

### Requirement: Durable advertisement store

Lilith's overlay advertisement store SHALL persist to disk and survive
restarts. Entries SHALL be served after restart until their TTL expires
absent re-confirmation. Store persistence MUST NOT record any information
about transient queriers (no logging of query sources into the store).

#### Scenario: Restart preserves cold-start service

- **WHEN** lilith restarts while a subnet's serving peers are offline
- **THEN** the persisted advertisements are still available to cold-start
  nodes that query afterward, within TTL bounds

### Requirement: Advertisement refinery

Lilith SHALL run a periodic refinery over its advertisement store,
verifying reachability of advertised addresses; entries failing checks
SHALL be downgraded or dropped. The refinery SHALL be rate-limited so it
does not dial a burst of addresses simultaneously.

#### Scenario: Dead advertisement expires early

- **WHEN** an advertised address fails refinery liveness checks before its
  TTL would expire
- **THEN** lilith stops serving that address before TTL expiry

### Requirement: Legacy per-network sections honored during migration

While the migration period is in effect, lilith SHALL keep accepting and
spawning per-network sections as independent network instances alongside
the overlay, preserving current seed behavior for apps that have not
adopted the swarm. Deprecation of per-network sections, when it comes,
SHALL be staged (warning first, refusal at a later release boundary).

#### Scenario: Mixed config runs both

- **WHEN** lilith is configured with both the overlay section and legacy
  per-network sections
- **THEN** the overlay seed and the legacy per-network instances all run

### Requirement: RPC reporting of overlay state

Lilith's JSON-RPC SHALL expose overlay seed status: listener health,
participating subnets, and advertisement store statistics. The reported
data MUST NOT include addresses or identifiers of transient queriers.

#### Scenario: Operator inspects seed

- **WHEN** an operator calls the lilith status RPC
- **THEN** listener health, known subnet identifiers, and advertisement
  counts are returned, with no record of which peers queried which subnets
