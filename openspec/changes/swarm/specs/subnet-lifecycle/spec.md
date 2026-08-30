## Purpose

Defines how an application uses the swarm overlay to manage subnets
dynamically: joining a subnet by resolving its identifier to reachable
peers, serving a subnet with advertisement obligations, leaving a subnet,
and keeping per-subnet state isolated — at runtime, without per-subnet seed
configuration.

## ADDED Requirements

### Requirement: Subnet join via overlay resolution

An application SHALL be able to join a subnet given only its descriptor: it
derives the subnet identifier, resolves the identifier to advertised
addresses over the overlay, seeds those addresses into the subnet's
unverified (greylist) peer set, and dials peers as an ordinary network
instance using the subnet's own magic bytes and application identity. The
overlay connection used for resolution MUST NOT become a connection of the
joined subnet. If the overlay yields no reachable addresses, joining SHALL
fail with a resolvable error rather than hang.

#### Scenario: Descriptor-only join

- **WHEN** an application requests joining a subnet for which it holds only
  the descriptor
- **THEN** the subnet network instance starts and establishes peer
  connections using addresses discovered over the overlay, without any
  subnet-specific seed configuration

#### Scenario: Unknown subnet

- **WHEN** no advertised address exists or none is reachable for a requested
  subnet
- **THEN** the join attempt reports failure in bounded time and no subnet
  network instance is left running

### Requirement: Per-subnet state isolation

Each subnet managed through the swarm SHALL have its own peer hostlists,
refinery behavior, and on-disk datastore and hostlist files, namespaced by
the subnet identifier. State of one subnet (hostlist entries, datastore,
refinement outcomes) MUST NOT be shared with or leak into another subnet
managed by the same node, and removing a subnet SHALL be possible without
damaging other subnets' persisted state.

#### Scenario: Independent refinement

- **WHEN** an address is unreachable in subnet A but healthy in subnet B
- **THEN** refinement in A downgrades it there while B's state for the same
  address is unaffected

#### Scenario: Namespaced persistence

- **WHEN** two subnets persist hostlists and datastores on one node
- **THEN** their files are stored under distinct paths derived from their
  subnet identifiers

### Requirement: Serving a subnet is opt-in with advertisement obligations

An application SHALL be able to declare, per subnet, that it serves the
subnet. Serving requires inbound addresses for that subnet; when active,
the node SHALL gossip advertisements for the subnet on the jittered cadence
defined by the overlay capability, and those advertisements SHALL carry
only the inbound addresses assigned to that subnet. A node that merely
joins a subnet (default) SHALL emit no advertisement for it. A transient
overlay node SHALL NOT serve any subnet.

#### Scenario: Opt-in serving advertises

- **WHEN** an application marks a joined subnet as served with configured
  inbound addresses
- **THEN** other overlay participants can resolve the subnet to those
  addresses, and no advertisement is emitted before the first cadence tick

#### Scenario: Join-only stays silent

- **WHEN** an application joins a subnet without declaring serving
- **THEN** no advertisement naming that subnet is ever emitted by the node

### Requirement: Subnet leave without departure broadcast

Leaving a subnet SHALL stop the subnet's network instance, release its
connections, and deregister it from the managing swarm. The leave MUST NOT
emit any departure or withdrawal message on the overlay; the node's
advertisements for that subnet simply cease and expire by TTL. Persisted
per-subnet state MAY be retained for rejoining.

#### Scenario: Silent leave

- **WHEN** an application leaves a subnet it was serving
- **THEN** no overlay message announces the departure, and other nodes'
  advertisement stores still list its addresses until TTL expiry or failed
  liveness checks remove them

#### Scenario: Rejoin reuses state

- **WHEN** a node rejoins a subnet it previously left and retained state for
- **THEN** its subnet peer hostlist resumes from the persisted state

### Requirement: Runtime subnet lifecycle

A swarm-managed node SHALL support starting and stopping subnets at any time
after the overlay is running, without restarting the overlay or other
subnets. Concurrent shutdown of multiple subnets SHALL terminate cleanly
without affecting surviving subnets.

#### Scenario: Late subnet spawn

- **WHEN** a new subnet join is requested while other subnets are already
  running
- **THEN** the new subnet starts without disruption to existing subnet
  connections or the overlay

#### Scenario: Concurrent teardown

- **WHEN** several subnets are stopped at once
- **THEN** all stop cleanly and remaining subnets continue operating

### Requirement: Pinned and private subnets

Applications SHALL be able to pin known subnet descriptors (e.g. released
networks) so their identifiers are stable across releases, and SHALL be able
to construct subnets from user-supplied descriptors, including
secret-bearing descriptors for private subnets. Static per-subnet seed lists
SHALL remain usable as a fallback or override for any pinned subnet during
the migration period.

#### Scenario: Pinned identifier matches released network

- **WHEN** an app pins a released network's descriptor
- **THEN** the derived identifier matches the one used by all other pinned
  deployments, and the subnet is discoverable through the overlay

#### Scenario: Static seeds still work

- **WHEN** an app is configured with static seeds for a pinned subnet and
  overlay discovery is unavailable
- **THEN** the subnet can still be joined via the static seeds
