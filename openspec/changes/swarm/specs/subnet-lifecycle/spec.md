## Purpose

Defines bounded, failure-safe application lifecycle behavior for resolving,
initializing, joining, creating, serving, recreating, and stopping isolated
swarm-managed subnets.

## ADDED Requirements

### Requirement: Descriptor-based resolution uses untrusted candidates

An application SHALL request a subnet using a valid descriptor, not a raw ID.
The swarm SHALL derive the normative ID and request only addresses for it.
Overlay results SHALL enter only an ephemeral typed unverified-target set for
that subnet, preserving original URL and validated socket until compatibility.
They MUST NOT enter URL-only persistent host/refinery state before success.
Overlay and subnet connections MUST remain separate. Every subnet peer MUST
independently pass magic-byte, application-name, and major/minor checks.
An overlay channel MUST NOT be reused, transferred, re-handshaken, or
multiplexed for subnet traffic. Default transient policy retains the overlay for
the application session; immediate post-lookup/join stop requires explicit
reduced-privacy policy and occurs after every caller-visible terminal outcome,
not an internal lookup phase within join. Any overlay stop MUST NOT stop the
independent subnet.

Resolution and connection phases SHALL have finite deadlines. A complete join
deadline MUST be configurable and no greater than 900 seconds.
Every untrusted candidate SHALL pass the overlay's resolution-time egress
policy before connection and consume the configured resolution/dial concurrency,
rate, per-destination, and total-candidate budgets. Rejected destinations SHALL
remain fallible candidate failures and MUST NOT trigger a second resolution in
the connector. Direct candidates SHALL retain the exact validated destination
socket. Tor/I2P candidates SHALL retain the exact trusted locally configured
proxy socket plus canonical hidden-service destination, with no local DNS or
advertisement-selected proxy. Every accepted transport path SHALL reject
malformed/unsupported input without unwind before or during compatibility.

Candidate ordering SHALL use two independently `OsRng`-shuffled bounded tiers:
at most 256 URLs from the bounded previously-compatible retry index and a
bounded reservoir sampled across the fresh terminal traversal through completion
or its fixed 16-page cap.
Persisted peers MUST be re-resolved and revalidated. After both tiers exist, the
candidate-preparation subdeadline SHALL use at most half the then-remaining
overall time and split resolution/validation time equally between tiers; unused
persisted time MAY pass to fresh, not conversely. A two-phase pre-start target
plan SHALL then split remaining dial time at a monotonic midpoint: persisted
targets stop/cancel by it and consume no more than their configured limit or half
the attempt budget; fresh targets activate for the second half. URL, wire, store,
hash, DNS-answer, and lexical order MUST NOT choose either attempted prefix.
Fresh candidates MUST NOT be persisted before compatibility succeeds.

#### Scenario: Descriptor-only resolution

- **WHEN** lookup returns a compatible reachable peer
- **THEN** the swarm attempts it through a distinct subnet connection

#### Scenario: Wrong-subnet candidate

- **WHEN** a candidate fails any compatibility field
- **THEN** it remains unverified and failure is not attributed to the relay

#### Scenario: Unknown subnet

- **WHEN** no selected source yields a compatible ordinary peer
- **THEN** join fails within its deadline and leaves no attempt running

#### Scenario: Candidate resolves to prohibited destination

- **WHEN** an advertised clearnet candidate resolves to loopback/private/
  reserved space outside explicit local-test mode
- **THEN** it is rejected before dialing and join continues within its budgets

#### Scenario: Hidden-service candidate enters subnet connector

- **WHEN** a canonical Tor/I2P candidate is selected for connection
- **THEN** the connector uses the trusted configured proxy socket, passes the
  hidden name only in proxy/TLS protocol, and has no direct fallback

#### Scenario: Stored ordering is adversarial

- **WHEN** returned URLs or persisted peers are arranged to control lexical or
  insertion order
- **THEN** bounded reservoir selection/CSPRNG shuffles choose attempted prefixes
  and the verified tier cannot consume the fresh tier's reserved time or budget

#### Scenario: Default join completion retains overlay

- **WHEN** a default-policy transient completes an ordinary subnet join
- **THEN** join completion does not itself stop the overlay

#### Scenario: Explicit immediate overlay stop after join

- **WHEN** reduced-privacy policy observes successful, failed, timed-out, or
  cancelled join completion
- **THEN** subnet network/app state continue and the caller was warned about
  timing correlation

### Requirement: Fallible initialization precedes network activity

Before each subnet attempt starts, the swarm SHALL invoke a fallible app
initializer with that attempt's `P2p` handle. It SHALL construct subnet-scoped
app state, register protocols, and return both caller-visible state and a
bounded shutdown hook. No listener, connection, or protocol job SHALL start
before success.

The registry SHALL retain ownership of the returned app state and shutdown hook
for the active attempt's entire lifetime; dropping the caller handle MUST NOT
drop required state. If initialization fails or is cancelled, partial state is
released, nothing starts, and the original error is returned.

#### Scenario: App state precedes connection

- **WHEN** initialization succeeds
- **THEN** protocols and registry-owned app state exist before network start

#### Scenario: Caller drops handle

- **WHEN** a caller drops its returned app handle while the subnet remains
  active
- **THEN** registry ownership keeps required app state alive

#### Scenario: Initializer fails

- **WHEN** initialization returns an error
- **THEN** no network activity starts and partial state is released

### Requirement: Join completion requires an ordinary persistent channel

`P2p::start()` alone SHALL NOT complete join. Join succeeds only after an
ordinary inbound, outbound, or manual subnet channel passes the compatibility
handshake and remains registered as an ordinary peer. Temporary direct, seed,
and refinement channels MUST NOT complete join, even when their handshake
succeeds.

Until then the registry SHALL expose a distinct joining state. Timeout,
cancellation, or candidate exhaustion SHALL stop tasks and connections, run
the shutdown hook, remove the attempt, and return a typed error without
panicking on untrusted input.

#### Scenario: Seed channel does not complete join

- **WHEN** seed discovery completes but no ordinary channel exists
- **THEN** join remains pending

#### Scenario: Refinement channel does not complete join

- **WHEN** a refinement probe succeeds but no ordinary channel exists
- **THEN** join remains pending

#### Scenario: Ordinary channel completes join

- **WHEN** a compatible ordinary channel completes before deadline
- **THEN** state atomically becomes joined

#### Scenario: Failed join rolls back

- **WHEN** deadline, cancellation, or failure ends an attempt
- **THEN** its state is removed without disturbing overlay or other subnets

### Requirement: Source policies use explicit bounded attempts

Applications SHALL select overlay-only, static-only, combined, or
overlay-then-static behavior. Sources MUST NOT activate implicitly outside the
selected policy.

- Overlay-only SHALL use overlay candidates with static seeds absent.
- Static-only SHALL use configured subnet seed discovery with overlay lookup
  absent.
- Combined MAY configure both sources in one attempt.
- Overlay-then-static SHALL complete and tear down one bounded overlay-only
  attempt before creating a fresh static-only `P2p` attempt. It SHALL rerun the
  initializer for the fresh attempt and remain inside one overall deadline.

No policy SHALL rely on runtime manual- or seed-session reload. Every source
uses the same compatibility and ordinary-channel completion rule.

#### Scenario: Overlay-first succeeds

- **WHEN** an ordinary overlay-discovered peer completes in the first attempt
- **THEN** no static attempt starts

#### Scenario: Overlay-first fails and static succeeds

- **WHEN** the first attempt fully rolls back under overlay-then-static
- **THEN** a fresh static-configured attempt reruns initialization and may join

#### Scenario: First-attempt state does not leak

- **WHEN** overlay-then-static creates its second attempt
- **THEN** no task, channel, app state, or registry ownership from the first
  attempt remains

#### Scenario: Every source fails

- **WHEN** all policy attempts fail within the overall deadline
- **THEN** a typed aggregate failure is returned after complete rollback

### Requirement: Per-subnet state remains isolated

Each subnet SHALL have independent hostlists, refinement, app state, tasks,
datastore, and hostlist files. Persistent paths SHALL be namespaced by full ID
under a configured root. No state, address outcome, dispatch, or shutdown signal
may cross subnets. Deleting one subnet MUST NOT alter another.

Overlay role persistence rules apply only to overlay-owned state. Applications
MAY separately configure subnet persistence for a transient overlay participant;
such paths reveal local subnet history and MUST be documented. Private secrets
MUST NOT be logged.

#### Scenario: Independent refinement

- **WHEN** one address has different outcomes in A and B
- **THEN** each subnet retains only its own outcome

#### Scenario: Namespaced persistence

- **WHEN** two subnets persist under one root
- **THEN** their files occupy distinct full-ID paths

#### Scenario: Transient persists a subnet explicitly

- **WHEN** a transient overlay caller enables subnet persistence
- **THEN** only that separately configured subnet state is written and its
  local-history implication is documented

### Requirement: Serving mode is selected before initial start

A subnet SHALL be created either join-only or serving. Serving creation SHALL
require a persistent overlay role, at least one local listener bind address,
and at least one externally advertised endpoint assigned to that subnet. Bind
addresses and advertised endpoints SHALL be separate fields and MUST NOT be
assumed identical. All addresses SHALL be validated before network start.

Serving initialization SHALL configure listeners before `P2p::start()`. It
completes when app initialization succeeds and required listeners bind; it does
not require an existing peer. This permits the first member of a new subnet to
serve. It may attempt peer discovery afterward under an explicit source policy.
A transient SHALL reject serving before any network activity.

Before initializer, listener, or author activation, serving SHALL atomically
allocate or reuse one 256-slot local-author reserve partition for the subnet.
If all configured partitions are occupied by subnets with protected local IDs,
serving SHALL fail with a typed capacity error. A newly allocated empty partition
SHALL be released on pre-authoring initialization/bind failure. Stopping serving
SHALL retain a nonempty partition until all protected local IDs expire, then
release it only if the subnet has not resumed serving.

#### Scenario: First serving member

- **WHEN** no subnet peer exists but a persistent caller supplies valid bind
  and advertised addresses
- **THEN** the serving subnet succeeds after listener readiness without a peer
  handshake

#### Scenario: Bind and advertised endpoint differ

- **WHEN** an externally provisioned endpoint forwards to a distinct local bind
  address
- **THEN** the listener binds locally while ads contain only the external
  endpoint

#### Scenario: Transient serving is rejected

- **WHEN** a transient requests serving
- **THEN** failure occurs before initializer, listener, or author task starts

#### Scenario: Author reserve is exhausted

- **WHEN** a persistent caller requests serving while every reserve partition is
  retained by protected local IDs for other subnets
- **THEN** failure occurs before initializer, listener, or author task starts

### Requirement: Serving promotion uses controlled recreation

An already started join-only subnet MUST NOT be promoted by mutating inbound
settings and calling session reload. Promotion SHALL require a controlled
stop/recreate operation: stop the joined instance completely, then create a new
serving-configured instance and rerun initialization. The operation SHALL be
serialized with other lifecycle actions and return a typed result if recreation
fails. Retained state MAY be reused only from that subnet's namespace.

Listener binds and externally advertised endpoints MUST remain distinct.
Swarm SHALL NOT claim automatic Tor or I2P identity provisioning. Reusing one
advertised endpoint across local subnets SHALL produce an explicit linkability
warning.

#### Scenario: Promotion does not use reload

- **WHEN** a joined subnet is promoted to serving
- **THEN** its old instance fully stops before a serving-configured instance
  starts

#### Scenario: Recreation fails

- **WHEN** the serving listener cannot bind or initialization fails
- **THEN** no partial serving instance or author task remains

#### Scenario: Shared endpoint warning

- **WHEN** one external endpoint is assigned to two served subnets
- **THEN** an explicit local linkability warning is produced

### Requirement: Advertisement authoring is serving-only and cadence-only

Join-only subnets SHALL author no ads. A successfully serving subnet SHALL
author bounded ads only on the overlay jittered cadence and only with its
advertised endpoints. Initialization, listener readiness, peer connection,
recreation, and new overlay channels MUST NOT emit immediately. Stopping
serving SHALL cease future authoring without a withdrawal.

#### Scenario: Join-only remains silent

- **WHEN** a subnet is joined without serving mode
- **THEN** it authors no ad

#### Scenario: First server waits

- **WHEN** a first serving member becomes listener-ready
- **THEN** its first ad waits for the next cadence tick

#### Scenario: Stop is silent

- **WHEN** a serving subnet stops
- **THEN** no departure message is sent and existing ads expire locally

### Requirement: Leave and recreation are idempotent and failure-safe

Leave SHALL disable authoring, stop network producers/channels/protocol jobs,
run app shutdown, and remove registry ownership. It MUST emit no withdrawal.
Repeated leave SHALL be idempotent. Same-subnet join, serve, recreate, leave,
and delete operations SHALL have one serialized owner.

The caller SHALL choose to retain or delete namespaced state after shutdown.
Deletion MUST occur only after stop and affect only that ID.

#### Scenario: Repeated leave

- **WHEN** leave is called after stop
- **THEN** it returns the already-stopped result without restarting work

#### Scenario: Retained rejoin

- **WHEN** state is retained
- **THEN** a later attempt may reuse only that subnet's files

#### Scenario: Delete after stop

- **WHEN** deletion is selected
- **THEN** only that namespace is deleted after shutdown

### Requirement: Concurrent lifecycle remains isolated

Different subnets MAY transition concurrently; one ID SHALL have one owner.
Duplicate joins MUST NOT create duplicate instances. Swarm shutdown SHALL stop
authoring, cancel attempts, stop every subnet despite individual failures, and
stop the overlay last within a finite deadline.

#### Scenario: Duplicate concurrent join

- **WHEN** two callers request the same descriptor
- **THEN** at most one instance is created and deterministic state is returned

#### Scenario: Late subnet operation

- **WHEN** a new join or serving creation starts while others run
- **THEN** existing subnet and overlay operation is not restarted

#### Scenario: One shutdown hook fails

- **WHEN** one app shutdown hook returns an error
- **THEN** every other subnet still receives a stop attempt and failures are
  aggregated without panic

### Requirement: Pinned and private descriptors share lifecycle rules

Applications SHALL pin public descriptors with golden IDs and accept valid
private descriptors. Private secrets MUST NOT be logged, advertised, sent in
lookup, or exposed by public status; overlay messages use only the derived ID.
Possession of the descriptor SHALL NOT bypass compatibility or app-level
authorization.

#### Scenario: Pinned interoperability

- **WHEN** deployments use one pinned descriptor
- **THEN** they derive one ID and apply the same compatibility checks

#### Scenario: Private descriptor is not transmitted

- **WHEN** a private subnet is resolved or created
- **THEN** no overlay message contains its secret

#### Scenario: Private ID grants no access

- **WHEN** a peer knows an ID but fails application authorization
- **THEN** swarm grants no authorization
