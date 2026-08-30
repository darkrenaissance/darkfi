## Purpose

Defines the swarm overlay rendezvous protocol: how subnet identifiers are
derived, how subnet advertisements are formatted, gossiped, stored, and
queried, the persistent vs transient participant roles, and the binding
anti-linkability constraints that keep the overlay from becoming a
cross-subnet correlation point.

## ADDED Requirements

### Requirement: Subnet identifier derivation

A subnet identifier SHALL be the BLAKE3 hash of a canonical, deterministically
serialized descriptor binding the application name, network magic bytes, and
version constraint of the subnet. The descriptor MAY include a secret; when
present, the resulting identifier SHALL be computationally indistinguishable
from any other identifier to a party lacking the secret. Two different
descriptors MUST NOT yield the same identifier.

#### Scenario: Same descriptor, same identifier

- **WHEN** two nodes derive an identifier from byte-identical descriptors
- **THEN** both obtain the same BLAKE3 identifier

#### Scenario: Private subnet is unguessable

- **WHEN** a party without the secret attempts to name or enumerate a
  secret-bearing subnet
- **THEN** they cannot produce its identifier or distinguish it from the
  identifier space of public subnets

#### Scenario: Divergent descriptors collide

- **WHEN** two descriptors differing in any bound field are hashed
- **THEN** the derived identifiers differ

### Requirement: Advertisement format and invariants

A subnet advertisement SHALL carry exactly one subnet identifier, a list of
`(address, last_seen)` pairs for peers serving that subnet, and a TTL in
seconds. Advertisements MUST be unsigned and MUST NOT contain any node
identity, key, or per-node nonce. Advertised addresses SHALL be restricted to
publicly shareable transport schemes. An advertisement for one subnet MUST
NOT embed addresses belonging to another subnet.

#### Scenario: Advertisement is self-contained per subnet

- **WHEN** a node gossips an advertisement for subnet S
- **THEN** the advertisement contains only S's identifier and addresses
  attributed to S, and no field links it to any other subnet or to a stable
  node identity

#### Scenario: Non-shareable address rejected

- **WHEN** an advertisement contains an address whose scheme is not publicly
  shareable (e.g. a proxy-internal scheme)
- **THEN** receiving nodes discard that address rather than storing or
  relaying it

### Requirement: Gossip propagation with origin ambiguity

Advertisements SHALL propagate by flooding/gossip between overlay peers. A
node relaying an advertisement MUST relay it unmodified, such that the
immediate sender of an advertisement is never evidence of its authorship.
Nodes MUST NOT include provenance, hop counts tied to a node, or relay
signatures in relayed advertisements. Re-gossip of a node's own served
subnets SHALL occur on a slow, jittered cadence and MUST NOT be triggered
immediately upon a subnet starting, an inbound listener appearing, or a new
overlay connection being established.

#### Scenario: Relay preserves ambiguity

- **WHEN** a node receives an advertisement and relays it to its peers
- **THEN** the relayed message is byte-equivalent in identifying fields and
  provides no indicator of whether the relaying node authored it

#### Scenario: No event-triggered advertisement

- **WHEN** a node begins serving a subnet or gains a new overlay peer
- **THEN** it does not immediately emit an advertisement for that subnet;
  the next emission waits for the jittered cadence

### Requirement: Subnet queries answered from local state

The overlay SHALL provide a query for the set of known subnet identifiers and
a query for the advertised addresses of a specific subnet. Both queries SHALL
be answered from the answering node's local advertisement state at the time
of the query, for every connected overlay peer regardless of that peer's
declared role. Query exchanges MUST NOT require the querier to reveal which
subnets it participates in beyond the subnet explicitly named in a
per-subnet query.

#### Scenario: Subnet list query

- **WHEN** a node sends a subnet-list query to a connected overlay peer
- **THEN** it receives the set of subnet identifiers currently known to that
  peer's local advertisement state

#### Scenario: Per-subnet address query

- **WHEN** a node sends a per-subnet query for identifier S
- **THEN** it receives addresses advertised for S from the answering peer's
  local state, and the exchange names no other subnet

### Requirement: Advertisement store with TTL and bounds

A persistent overlay node SHALL maintain an advertisement store. Entries
SHALL expire no later than their TTL after last confirmation. Persistent
nodes SHALL verify reachability of advertised addresses on a periodic,
liveness-check schedule and SHALL drop or downgrade entries that fail.
The store SHALL enforce per-subnet and total entry caps so that
advertisement floods cannot grow it unboundedly. Transient nodes SHALL NOT
maintain a persistent store.

#### Scenario: TTL expiry

- **WHEN** an advertisement entry's TTL elapses without re-confirmation
- **THEN** the store no longer returns that entry in query responses

#### Scenario: Unreachable advertisement dropped

- **WHEN** an advertised address repeatedly fails liveness checks
- **THEN** the store stops serving it and it is eligible for removal

#### Scenario: Flood bounded

- **WHEN** an attacker floods advertisements exceeding the store caps
- **THEN** store size stays within its configured bounds

### Requirement: Overlay node roles — persistent and transient

An overlay node SHALL declare itself persistent by advertising a
swarm-ad-store feature in the connection handshake's features vector; a node
not advertising it is transient. A transient node SHALL NOT accept inbound
overlay connections, SHALL NOT emit subnet advertisements, and SHALL NOT
maintain an advertisement store. A transient node MAY persist a cache of
overlay peer addresses (an overlay hostlist) so later sessions dial cached
overlay peers before configured seeds; such a cache MUST NOT record which
subnets the node queried or joined. A persistent node SHALL maintain a
TTL-bounded advertisement store and SHALL relay gossip. Both roles SHALL
handle all overlay protocol messages identically while connected; role
SHALL NOT alter message formats or per-message processing, only the local
state available to answer from. No overlay node SHALL refuse
protocol-correct messages solely because the sender is transient.

#### Scenario: Mobile lookup session

- **WHEN** a transient node connects to the overlay, resolves the addresses
  of a subnet, and disconnects
- **THEN** it has persisted at most its overlay hostlist cache (overlay
  peer addresses only, no subnet identifiers), has emitted no
  advertisements, retains no advertisement store, and appears in no other
  node's hostlist or advertisement store

#### Scenario: Cached bootstrap avoids seeds

- **WHEN** a transient node starts a session with a populated overlay
  hostlist cache while its configured seeds are unreachable
- **THEN** it still establishes overlay connectivity by dialing cached
  overlay peers

#### Scenario: Persistent node cold-starts another

- **WHEN** a fresh node connects to any persistent overlay node and queries
  a subnet
- **THEN** it can discover and dial serving peers without any per-subnet
  seed configuration

#### Scenario: Role does not split protocol behavior

- **WHEN** the same query is sent to a persistent and to a transient node
  that both hold the same local advertisement state
- **THEN** both answer with the same message types and semantics

### Requirement: Metered overlay messages

All overlay protocol messages SHALL be subject to per-message-type metering
with configured thresholds and penalties, such that a peer flooding any
overlay message type is throttled or banned under the node's ban policy.
Message size estimates SHALL be declared for every overlay message type.

#### Scenario: Query flood throttled

- **WHEN** a peer sends subnet queries beyond the metering threshold
- **THEN** the node applies the metering penalty for that message type

### Requirement: No cross-subnet linkability in overlay traffic

The overlay protocol MUST NOT provide any mechanism that links a single
node across subnets: advertisements for different subnets served by one
node MUST NOT be correlatable by their content, and no overlay message
SHALL carry a stable node identifier, signature key, or address reused
across subnets. Deployment of a shared inbound endpoint across multiple
served subnets SHALL be flagged as a known linkability hazard in node
documentation.

#### Scenario: Two ads from one operator stay unlinkable

- **WHEN** one operator serves two subnets through distinct per-subnet
  addresses and follows the jittered cadence
- **THEN** an overlay observer cannot bind the two advertisements to each
  other through any field of the overlay protocol itself
