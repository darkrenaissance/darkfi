## Purpose

Defines a bounded rendezvous overlay that resolves known swarm descriptors to
untrusted peer addresses while making bootstrap, replay, resource, persistence,
and metadata-disclosure boundaries explicit.

## ADDED Requirements

### Requirement: Versioned swarm identifier derivation

A version-1 descriptor SHALL bind the swarm application name, magic bytes,
and major/minor version pair used by the existing compatibility handshake. Its
canonical bytes SHALL be, in order:

1. ASCII `darkfi-swarm-id-v1` followed by one zero byte;
2. application-name UTF-8 byte length as unsigned 16-bit big-endian, followed
   by those exact bytes without Unicode normalization;
3. the four magic bytes;
4. major and minor versions as unsigned 64-bit big-endian integers;
5. zero for a public descriptor or one for a private descriptor; and
6. for a private descriptor only, exactly 32 secret bytes.

Application names MUST contain 1 through 32 UTF-8 bytes. Other private-secret
lengths and flag values MUST be rejected. `SwarmId` SHALL be the 32-byte
BLAKE3 hash of the canonical bytes. Implementations SHALL rely on collision
resistance rather than claim collision is impossible. Patch, prerelease, and
build metadata SHALL NOT affect the ID.

For public `darkirc`, magic `fb e5 c7 b5`, and compatibility pair `0.5`, the
canonical bytes SHALL be:

`6461726b66692d737761726d2d69642d76310000076461726b697263fbe5c7b50000000000000000000000000000000500`

and `SwarmId` SHALL be:

`cad1d01807849a400541725bd75af14e53d259392082c14fa96ce6313551feb4`.

#### Scenario: Golden public identifier

- **WHEN** an implementation derives the specified darkirc identifier
- **THEN** its canonical bytes and hash equal the normative values

#### Scenario: Compatibility-equivalent patch versions

- **WHEN** descriptors differ only in patch, prerelease, or build metadata
- **THEN** they derive the same identifier

#### Scenario: Bound field differs

- **WHEN** valid descriptors differ in any bound field
- **THEN** their canonical bytes differ and identifiers are expected to differ
  under BLAKE3 collision resistance

#### Scenario: Malformed private descriptor

- **WHEN** a private descriptor secret is not exactly 32 bytes
- **THEN** validation fails before hashing

### Requirement: Private identifiers are rendezvous capabilities only

A generated private secret MUST use a cryptographically secure random source.
A caller-supplied secret SHALL be accepted only as exactly 32 bytes and SHALL
be documented as requiring independent high entropy. It makes an identifier
difficult to derive only before observation. It MUST NOT be represented as
authentication, encryption, authorization, or continuing confidentiality.

Public enumeration MUST omit records from accepted ads marked non-public, but
visibility is unauthenticated sender data rather than an intrinsic property of
an ID. Persistent gossip peers necessarily observe IDs they store or relay, and
an attacker that learns an ID can submit another ad marking it public.

#### Scenario: Unobserved random private identifier

- **WHEN** a party neither knows nor observes a uniformly random secret
- **THEN** deriving its identifier requires guessing the 32-byte secret

#### Scenario: Private identifier is disclosed

- **WHEN** a private ID is sent in an ad or lookup
- **THEN** the receiving overlay peer can observe and reuse it

#### Scenario: Observed private ID is relabeled

- **WHEN** an attacker re-advertises an observed private ID with public
  visibility
- **THEN** the protocol may catalog the forged public record and makes no
  intrinsic-privacy claim for the ID

### Requirement: Bootstrap uses staged ordinary non-seed overlay connections

Bootstrap addresses SHALL use ordinary non-seed sessions capable of carrying
swarm requests and responses, not existing short-lived seed sessions.
“Ordinary” SHALL describe session type, not a requirement that a transient keep
the channel indefinitely. A transient with a cache SHALL first construct and
start an overlay instance using cached peers only. If no ordinary compatible
channel completes within the stage timeout, it SHALL fully stop and discard
that instance before constructing a fresh overlay instance using configured
ordinary bootstrap peers. It MUST NOT depend on runtime manual-session reload.

Each stage timeout MUST be finite and no greater than 300 seconds. The cache
MUST contain no more than 256 endpoint records, 262,144 encoded file bytes, or
1,024 encoded bytes per URL. A record SHALL contain only the exact canonical
connect URL and resolved endpoint actually used by a successfully completed
ordinary channel advertising `("swarm-ad-store", 1)`. Advertised external
addresses MUST NOT be cached from that feature handshake. The cache MUST NOT
contain features, swarm IDs, ads, queries, or source/query associations.

#### Scenario: Cached stage succeeds

- **WHEN** a cached ordinary peer completes before the stage deadline
- **THEN** configured bootstrap peers are not contacted

#### Scenario: Cached stage fails

- **WHEN** no cached peer completes before the stage deadline
- **THEN** the cached overlay instance is stopped before a fresh configured-peer
  instance starts

#### Scenario: Bootstrap peer answers lookup

- **WHEN** a fresh configured-peer stage establishes an ordinary channel
- **THEN** lookup can run on that channel without a seed session

#### Scenario: Cache contains no swarm activity

- **WHEN** lookup sessions persist the swarm cache
- **THEN** it contains successful connect/resolved endpoint pairs only

#### Scenario: Advertised external address is not cache authority

- **WHEN** a connected persistent peer advertises an external address different
  from the endpoint used by its successful channel
- **THEN** the different external address is not added to the cache

### Requirement: Untrusted dials use validated exact targets

Before dialing a fresh advertised or configured untrusted target, the joiner
SHALL resolve clearnet names once and validate the selected socket address. A
cached target SHALL instead revalidate and reuse its stored successfully
connected socket without DNS resolution. Unless explicit local-test mode is
enabled, both paths MUST reject loopback, private, shared, link-local, multicast,
unspecified, documentation, benchmarking, and otherwise reserved destinations
for IPv4 and IPv6. Production lilith MUST NOT enable local-test mode.

Every validated target SHALL carry the original URL, one mandatory exact socket,
and a direct or trusted-proxy route kind. For a direct route, the socket is the
validated destination and connection SHALL use it without a second DNS lookup;
the original hostname MAY be retained only for TLS identity.

For a trusted-proxy route, the mandatory socket SHALL be the exact endpoint from
trusted local proxy configuration, not the advertised destination. That proxy
socket MAY be loopback/private under local trust policy; an advertisement MUST
NOT select or override it. The untrusted destination SHALL be either a globally
routable IP literal or a canonical Tor/I2P hidden-service name matching the
transport. A hidden-service name MUST NOT undergo local DNS and MAY be passed
only inside proxy negotiation and for TLS identity. Arbitrary clearnet hostnames
MUST NOT be sent for remote proxy DNS. Missing/malformed proxy configuration and
transport/scheme mismatch SHALL reject the candidate. One join attempt SHALL
dial one exact route at most once.

A clearnet DNS answer SHALL contain at most 16 socket addresses and every answer
SHALL consume the join's resolution budget and pass address classification.
Empty or oversized answers SHALL fail. A bounded nonempty answer SHALL be
iterated without unchecked indexing; its allowed sockets SHALL be shuffled with
`OsRng` before selecting at most one exact socket for that URL. A cached trusted-
proxy route SHALL revalidate that its exact socket still matches current trusted
local proxy configuration; mismatch fails without DNS or fallback.

An advertised candidate SHALL remain a typed pair of original URL and validated
resolved socket through the swarm connection attempt. Before compatibility
succeeds it MUST NOT be downgraded into a URL-only hostlist, persisted, or sent
through a connector/refinery path that resolves it again. Failed candidates
SHALL be dropped. After compatibility succeeds, ordinary host persistence MAY
record the peer URL, but every future reconnect or refinement attempt MUST
resolve, validate, budget, and dial one exact socket again.

#### Scenario: DNS result changes after validation

- **WHEN** a hostname changes resolution after an address was validated
- **THEN** the connection uses the previously validated socket and performs no
  second DNS lookup

#### Scenario: Cached hostname would resolve differently

- **WHEN** a cached record contains an original hostname and successful socket
  but DNS now returns another address
- **THEN** cache reuse revalidates/dials the stored socket and performs no DNS
  lookup for that cached attempt

#### Scenario: Private or loopback result

- **WHEN** an untrusted clearnet target resolves to a prohibited range outside
  explicit local-test mode
- **THEN** it is rejected before connection

#### Scenario: Victim endpoint is repeatedly advertised

- **WHEN** many ads name one globally routable victim endpoint
- **THEN** one join attempt dials it at most once and all dial rate/concurrency/
  total budgets remain enforced

#### Scenario: Remote proxy DNS target

- **WHEN** an untrusted ad supplies an arbitrary clearnet hostname to a proxy
  mode that would resolve it remotely
- **THEN** the target is rejected rather than bypassing egress validation

#### Scenario: Canonical hidden service uses trusted proxy

- **WHEN** an accepted Tor/I2P candidate names a canonical hidden service
- **THEN** its exact socket is the locally configured trusted proxy, the hidden
  name is not locally resolved, and the ad cannot alter the proxy endpoint

#### Scenario: Hidden-service proxy is absent

- **WHEN** an otherwise valid hidden-service candidate has no valid configured
  proxy matching its transport
- **THEN** it fails before dialer construction without direct-network fallback

#### Scenario: Candidate crosses into swarm connector

- **WHEN** a validated advertised candidate is handed to the swarm connection
  attempt
- **THEN** its original URL and exact socket remain typed until compatibility
  succeeds, with no second DNS lookup or URL-only greylist insertion

#### Scenario: Failed candidate is not persisted

- **WHEN** a validated candidate fails transport or compatibility
- **THEN** it is dropped without entering persistent hostlist or refinery state

#### Scenario: Compatible peer reconnects later

- **WHEN** an ordinary persisted peer is retried or refined in a later attempt
- **THEN** that attempt resolves and validates a new exact target under the same
  egress and dial budgets before connection

### Requirement: Advertised-candidate processing is fully fallible

The complete attacker-selected candidate pipeline SHALL be fallible: URL parse,
scheme allowlist, host/port extraction, DNS result handling, address
classification, validated-target construction, proxy selection and negotiation,
transport/TLS dialing, and compatibility. It MUST NOT use `unwrap`, `expect`,
explicit panic, unchecked slicing/indexing, or an unimplemented transport branch.
Unsupported or unaudited schemes MUST be rejected before dialer construction.
Empty or oversized DNS results, malformed targets, missing/malformed proxies,
timeouts, cancellation, and transport errors MUST return bounded candidate
errors. Bounded multiple-address results MUST be iterated and classified without
unchecked selection. Processing SHALL continue or terminate only according to
the join budget.

#### Scenario: Resolution returns no addresses

- **WHEN** an advertised clearnet name resolves to an empty set
- **THEN** candidate processing returns an error without indexing or unwind

#### Scenario: Resolution returns multiple addresses

- **WHEN** a clearnet name resolves to a bounded nonempty address set
- **THEN** every result is budgeted/classified without unchecked indexing and at
  most one `OsRng`-shuffled allowed exact socket is selected for that URL

#### Scenario: Every accepted scheme receives hostile input

- **WHEN** arbitrary malformed targets exercise each scheme accepted from ads
- **THEN** parsing through compatibility returns bounded errors without unwind

#### Scenario: Enabled transport has no audited fallible adapter

- **WHEN** an advertisement selects an enabled but unsupported or unimplemented
  transport path
- **THEN** the candidate is rejected before transport construction or dialing

### Requirement: Overlay control channels are never swarm data channels

An overlay channel SHALL remain owned by the overlay `P2p` identified by fixed
swarm magic bytes, app name, version policy, host state, and protocol registry.
It MUST NOT carry swarm application messages, be transferred to a swarm
`P2p`, be re-handshaken in place under a swarm identity, or multiplex traffic
tagged by swarm ID. Joining SHALL create a separate swarm channel under that
swarm's own identity and state.

A persistent participant SHALL keep its overlay active while it performs
durable store or gossip duties. A participant serving any swarm SHALL keep the
overlay active while advertisement authoring is enabled. The default transient
policy SHALL retain the overlay for the application session and MUST NOT stop it
as an automatic reaction to lookup or join completion. The application session
ends only through explicit overlay stop or full SwarmPool shutdown. An explicit
reduced-privacy policy SHALL stop after every caller-visible lookup/join terminal
outcome—success, empty result, error, timeout, or cancellation—but MUST NOT stop
after an internal lookup phase within join. Configuration and documentation
MUST warn that responder/swarm observers can correlate the query, swarm
connection, and teardown timing. Stopping the overlay MUST NOT stop or transfer
the independent swarm `P2p`; a later lookup establishes a new overlay session
only when no active overlay remains.

#### Scenario: Overlay peer also serves requested swarm

- **WHEN** the lookup responder also operates a serving endpoint for the
  requested swarm
- **THEN** the client opens a separate swarm connection rather than reusing the
  overlay channel

#### Scenario: Default transient join does not trigger disconnect

- **WHEN** a default-policy transient completes lookup or swarm join
- **THEN** join completion itself does not stop the overlay

#### Scenario: Explicit reduced-privacy teardown

- **WHEN** a caller selects immediate teardown and lookup or join reaches any
  terminal outcome
- **THEN** the overlay stops without stopping the swarm and configuration/docs
  flag the timing-correlation risk

#### Scenario: Join's internal lookup completes

- **WHEN** reduced-privacy policy is active and an internal lookup yields
  candidates while the caller-visible join remains in progress
- **THEN** the overlay is not stopped until join reaches its terminal outcome

#### Scenario: Persistent duties retain overlay

- **WHEN** persistent store/gossip or serving-advertisement duties remain active
- **THEN** the participant does not intentionally stop its overlay

### Requirement: Wire messages have fixed correlation and size bounds

Every lookup request SHALL carry a fresh cryptographically random 16-byte
request ID. Its response or bounded error SHALL echo that ID. A channel SHALL
have no more than 32 outstanding requests. Responses with unknown, duplicate,
expired, or mismatched request IDs MUST be rejected. Pending request state MUST
be removed on response, timeout, or disconnect. Request timeout SHALL default
to 10 seconds, be configurable no higher than 60 seconds, and produce a local
timeout error; a late response is unsolicited and receives no wire error.

Protocol hard limits SHALL be:

| Message | Maximum encoded bytes |
|---|---:|
| `SwarmAd` | 65,536 |
| `GetSwarmAddrs` | 128 |
| `SwarmAddrs` | 65,536 |
| `GetPublicSwarms` | 128 |
| `PublicSwarms` | 16,384 |
| `SwarmError` | 128 |

Every encoded URL MUST be no more than 1,024 bytes. A page cursor SHALL be a
fixed 65-byte value encoded as version `u8`, last returned key `[32]`, and
terminal key `[32]`. Size/count validation MUST occur before store, index,
response, or relay work.

Message command strings and canonical field order SHALL be:

| Command | Fields in encoded order |
|---|---|
| `ad` | `SwarmId[32]`, visibility `u8`, ad ID `[32]`, lifetime `u32`, URL vector |
| `getaddr` | request ID `[16]`, `SwarmId[32]`, optional cursor |
| `addrs` | request ID `[16]`, `SwarmId[32]`, URL vector, optional cursor |
| `getswarm` | request ID `[16]`, optional cursor |
| `swarms` | request ID `[16]`, `SwarmId` vector, optional cursor |
| `err` | request ID `[16]`, error code `u8` |

Fields SHALL use existing DarkFi canonical wire encoding. Visibility values
SHALL be `0 = public` and `1 = non-public`; other values are invalid. Error
values SHALL be `0 = malformed`, `1 = invalid cursor`, `2 = enumeration
disabled`, and `3 = busy`; other values are invalid. Lifetime is unsigned
32-bit and cursor version SHALL be one.

Every attacker-controlled swarm, version, and verack decoder SHALL return a
fallible error for malformed or truncated input without unwind. Such paths MUST
NOT use `unwrap`, `expect`, explicit panic, unchecked slicing/indexing, or
reserve/allocate from an unvalidated declared length/count. Bounds validation
MUST precede allocation and element decoding.

#### Scenario: Concurrent requests correlate correctly

- **WHEN** multiple lookups are outstanding on one channel
- **THEN** each response completes only the request whose ID it echoes

#### Scenario: Unsolicited response

- **WHEN** a response carries no live matching request ID
- **THEN** it is rejected and consumes metering budget

#### Scenario: Message exceeds hard limit

- **WHEN** any message exceeds its encoded maximum
- **THEN** it is rejected before variable store or relay work

#### Scenario: Payload is truncated at any byte

- **WHEN** a valid swarm, version, or verack payload is truncated at any byte
  boundary
- **THEN** decoding returns an error without unwind or excessive allocation

#### Scenario: Declared length is hostile

- **WHEN** an arbitrary payload declares a count or length larger than its
  validated bound or remaining bytes
- **THEN** decoding rejects it before reservation, slicing, or element work

### Requirement: Advertisement format is bounded and nonempty

An ad SHALL contain exactly one swarm ID, public/non-public visibility, a
fresh 32-byte per-ad ID, lifetime seconds, and 1 through 32 serving addresses.
The ad ID MUST use a cryptographically secure random source and MUST NOT be
reused for another emission or swarm. It is deduplication data, not identity.

Lifetime MUST be 1 through 86,400 seconds. Ads MUST NOT contain `last_seen`, a
stable node ID, signing key, author, relay provenance, hop count, or identifier
shared with another swarm. Every address MUST be valid, publicly shareable,
and within the URL bound. A receiver SHALL reject the entire ad if any field or
address is invalid.

#### Scenario: Valid ad is swarm-scoped

- **WHEN** a valid ad for S is accepted
- **THEN** it contains only S, S's addresses, and a unique ephemeral ad ID

#### Scenario: Empty ad

- **WHEN** an ad contains no address
- **THEN** it is rejected without store or relay work

#### Scenario: Invalid address

- **WHEN** any ad address is malformed, non-shareable, or overlong
- **THEN** no part of the ad is stored or relayed

### Requirement: Expiry, replay suppression, and storage remain bounded

A persistent participant SHALL use finite nonzero per-swarm, global-address,
per-swarm protected-ID, and global protected-ID caps. Configured values MUST NOT
exceed 1,024 addresses per swarm, 65,536 total addresses, 1,024 protected IDs
per swarm in the general pool, or 262,144 protected IDs globally. Runtime
expiry SHALL use a monotonic deadline from local receipt and sender clocks SHALL
have no effect.

Those caps SHALL default respectively to 256 addresses per swarm, 16,384 total
addresses, 256 general protected IDs per swarm, and 65,536 protected IDs
globally.

The receiver SHALL clamp each accepted address lifetime to the lesser of the
wire lifetime and a local receive cap. That cap SHALL default to 7,200 seconds,
MUST be nonzero, and MUST NOT exceed 86,400 seconds. Local author lifetime SHALL
also default to 7,200 seconds and MUST NOT exceed 86,400 seconds.

Receiving a retained ad ID MUST NOT extend expiry or repeat relay work. The
dedup deadline SHALL be exactly 86,400 seconds after the associated locally
clamped address expiry, making total protection no greater than 172,800 seconds
from acceptance. A protected ID MUST NOT be evicted before that deadline. A fresh remote
ad SHALL be rejected without address mutation or relay when its swarm's
general-pool quota or the global general pool has no expired slot. Expired IDs
may be evicted deterministically.

Local-author reserve-swarm partitions SHALL default to 32 and MUST NOT exceed
256. Each partition contains exactly 256 protected-ID slots and counts within the
global cap; checked configuration arithmetic SHALL require a nonzero remaining
general pool. Remote ads MUST NOT consume reserve partitions. A serving
transition SHALL atomically allocate/reuse one partition for its swarm before
listener or author activation and fail with a typed capacity error when none is
available. Stopping service SHALL retain that partition until all its protected
local IDs expire, then release it atomically when that swarm is not serving, so
sequential swarm churn cannot overwrite protection.

Startup SHALL validate the partitions independently: general protected IDs MUST
fit the global general capacity and each general per-swarm quota; local IDs MUST
fit 256 slots for each distinct reserved swarm and the configured partition
count. Persisted local IDs already occupy their reserve and MUST NOT be counted
again as general state. Locally authored IDs MAY NOT evict protected IDs. The
reserve prevents remote admission from blocking allocated local cadence, but
does not provide preferential validation or remote role privilege.

Fresh IDs may refresh addresses subject to caps. Address eviction SHALL choose
expired entries first, then earliest expiry, then lexical key. Stores MUST NOT
dial advertised addresses or record ad sources, queriers, query history, or
source-peer/swarm associations. Replay IDs SHALL remain globally keyed; each
record SHALL bind its advertised swarm only for quota/protection accounting, so
reuse of one ad ID under another swarm is still a duplicate. A local-author
reserve record necessarily marks an ID as generated by this process; that local
fact and reserve occupancy/use/failure/timing MUST NOT enter wire messages,
RPC, status, metrics, telemetry, or peer-linked state, even as aggregate counters.
Fresh forged IDs can still poison within bounds; the store provides no authenticity.

Acceptance SHALL atomically commit the global seen-ID record, quota/reserve
accounting, address records, and public-index mutation before any relay job is
enqueued. Commit failure SHALL cause no mutation or relay. Rollback of the
database to a snapshot before that commit can remove the seen ID and permit a
later replay; this capability makes no non-rollbackable replay guarantee.

Store state SHALL be normalized per `(SwarmId, canonical address)`. Accepting
a fresh ad updates visibility and expiry for every address present in that ad;
addresses absent from it retain their current record until independently
updated, expired, or evicted. An ID is publicly enumerable iff at least one
live normalized record is marked public. Public-to-non-public and reverse
updates of the same address take effect atomically. Direct lookup returns all
live records regardless of visibility.

Persistent replay state SHALL use unsigned 64-bit monotonic epoch ticks in
seconds. Each seen-ID record stores its checked deadline tick, and one atomic
metadata checkpoint stores elapsed tick every 300 seconds by default, no less
often than every 600 seconds, and on clean shutdown. On restart, a record with
`deadline <= checkpoint` SHALL be expired without subtraction. Otherwise the
implementation SHALL use checked subtraction, reject a delta greater than
173,400 seconds as incoherent, clamp valid remaining duration to 172,800
seconds, and use checked duration conversion and `Instant` deadline addition.
Overflow, underflow, missing/incoherent epoch metadata, or failed deadline
construction SHALL return a typed startup error.

If a durable checkpoint cannot complete before the 600-second maximum interval,
the persistent store SHALL reject fresh ad acceptance and local authoring until
a checkpoint succeeds or controlled shutdown completes; it MUST NOT continue
creating deadline deltas outside the validated bound.

All surviving records and new epoch metadata SHALL replace the old epoch in one
atomic batch; interruption leaves the old epoch loadable. Downtime and the
uncheckpointed interval are not subtracted, so they MAY extend a record present
in the loaded database, but restart MUST NOT reset every such ID to a fresh full
horizon or shorten it. A database rollback before seen-ID commit can remove the
record entirely and is explicitly outside that guarantee. Address records
continue to use persisted wall expiry and MAY expire conservatively on clock
anomalies. If separately validated general/reserve capacities cannot contain
valid persisted protected IDs, startup SHALL fail without eviction.

Malformed or unverifiable persisted seen-ID, quota/reserve, or epoch state SHALL
fail startup. Malformed address records MAY be quarantined and a public index MAY
be rebuilt only when authoritative replay/accounting state remains intact.

#### Scenario: Duplicate does not refresh

- **WHEN** a retained ad ID is replayed
- **THEN** original expiry remains and no second relay occurs

#### Scenario: Ad ID is reused for another swarm

- **WHEN** a retained global ad ID appears with a different swarm ID
- **THEN** it remains a duplicate and does not consume that swarm's quota or
  mutate/relay addresses

#### Scenario: Protected dedup set is full

- **WHEN** a fresh ad arrives while every dedup slot is protected
- **THEN** the fresh ad is rejected instead of evicting a protected ID

#### Scenario: One swarm fills its protected-ID quota

- **WHEN** fresh remote ads for one swarm consume every unexpired slot in that
  swarm's general quota
- **THEN** another fresh ad for that swarm is rejected without consuming slots
  reserved for other swarms or local authoring

#### Scenario: Remote flood reaches the local-author reserve

- **WHEN** the remote general pool is full while local authoring remains active
- **THEN** a locally authored ad may use its swarm reserve and no remote ad may
  consume that slot

#### Scenario: Sequential serving exhausts reserve partitions

- **WHEN** stopped swarms with protected local IDs occupy every configured
  reserve partition and another swarm requests serving
- **THEN** transition fails before listener/author activation without evicting
  or shortening any occupied partition

#### Scenario: TTL expires without probe

- **WHEN** an address reaches local expiry without a fresh accepted ad
- **THEN** it is no longer returned and no address probe occurred

#### Scenario: Restart preserves address expiry conservatively

- **WHEN** durable state reloads before expiry with a non-rollback wall clock
- **THEN** only remaining lifetime is restored as a monotonic deadline

#### Scenario: Restart restores checkpointed protection

- **WHEN** a valid persisted seen ID loads after restart
- **THEN** its checked positive deadline/checkpoint difference is restored
  conservatively rather than resetting it to the full horizon

#### Scenario: Deadline equals checkpoint

- **WHEN** a persisted deadline tick is equal to or below the checkpoint tick
- **THEN** the record expires without unsigned subtraction or revival

#### Scenario: Epoch arithmetic is incoherent

- **WHEN** subtraction/addition would underflow/overflow or a stored delta
  exceeds 173,400 seconds
- **THEN** startup returns a typed error without clamping wrapped arithmetic

#### Scenario: Crash precedes the next checkpoint

- **WHEN** a process crashes less than 600 seconds after its last checkpoint
- **THEN** records present in the loaded database may be extended by the
  uncheckpointed interval and downtime but are not shortened

#### Scenario: Store rolls back before acceptance commit

- **WHEN** an operator restores a database snapshot predating an accepted ad ID
- **THEN** that ID may be accepted and relayed again, and documentation does not
  claim rollback-resistant replay suppression

#### Scenario: Reduced capacity cannot hold protected state

- **WHEN** configured dedup capacity is below valid persisted seen-ID count
- **THEN** startup fails without evicting a protected ID

### Requirement: Gossip is bounded without anonymity overclaim

Persistent participants SHALL relay each newly accepted ad without changing
identifying contents. Relay fanout MUST be finite and no greater than 64.
Queued relay work MUST be bounded and duplicates MUST NOT be requeued. Own ads
SHALL be authored only on a fixed 1,800-second base cadence with independently
sampled uniform jitter from -600 through +600 seconds. This cadence is not
configurable in version one. Authored lifetime SHALL default to 7,200 seconds
and MUST NOT exceed 86,400 seconds. Swarm start, listener start, and new overlay
channels MUST NOT trigger authoring.

A transient MAY relay newly accepted ads from bounded memory but SHALL NOT
author one. No author field means an immediate sender is not protocol-level
proof of authorship; this MUST NOT be described as hiding authorship against
timing, topology, first-seen, or global observation.

#### Scenario: Relay preserves contents

- **WHEN** an accepted ad is relayed
- **THEN** swarm ID, visibility, ad ID, lifetime, and addresses are unchanged

#### Scenario: Gossip loop

- **WHEN** the same ad returns during its protected horizon
- **THEN** no second relay is queued

#### Scenario: Start remains silent

- **WHEN** serving or a new overlay channel starts
- **THEN** authoring waits for the cadence

### Requirement: Lookup and optional public enumeration are paginated

Direct lookup SHALL name one swarm and return addresses only for it. Each page
SHALL echo the request ID, contain at most 64 addresses and 65,536 encoded
bytes, and include at most one fixed cursor. On the first page, the responder
SHALL capture the current greatest live ordered key as the terminal key. A next
cursor SHALL contain the last returned key and that fixed terminal key. Later
pages SHALL return only live keys strictly greater than the last key and no
greater than the terminal key, advancing the last key monotonically. A cursor
whose version, length, or key ordering is invalid SHALL return a bounded invalid-
cursor error. Index mutation MUST NOT invalidate a well-formed cursor, create a
server snapshot, or force traversal restart; it MAY cause records added/removed
during traversal to be included or omitted. Page limits still bound completion.
For every page, the responder SHALL derive canonical keys, return each key at
most once in strictly ascending order, and keep every key in
`(previous_last, terminal]` when a previous cursor exists. A next cursor MUST be
absent on an empty page and otherwise its last key MUST equal the greatest
returned key with `last < terminal`. The requester SHALL independently derive
and validate those keys, reject duplicates within/across pages, reject an empty
page with a next cursor, and reject a response cursor that changes the first
page's terminal, fails to advance, or disagrees with returned keys. Requester
dedup state remains bounded by page/item limits.

Public enumeration SHALL be disabled by default and require explicit local
enablement. If enabled, it SHALL return IDs having at least one live normalized
address record marked public, at most 256 IDs and 16,384 bytes per page. It SHALL
be available to every protocol-correct connected peer without role privilege and
MAY be disabled globally without disabling direct lookup.
Visibility is not authenticated: an attacker can cause an observed ID to
appear by submitting a public-marked address record. No response SHALL include
ad sources or querier data.

#### Scenario: Direct lookup is isolated

- **WHEN** addresses for S are requested
- **THEN** response pages contain S addresses only within both page bounds

#### Scenario: Non-public record is omitted

- **WHEN** an ID has only accepted non-public records
- **THEN** it is omitted while direct lookup remains possible to a caller
  already knowing the ID

#### Scenario: Enumeration setting is omitted

- **WHEN** an operator does not explicitly enable public enumeration
- **THEN** public-list requests return the bounded disabled error while direct
  lookup remains available

#### Scenario: Index changes during pagination

- **WHEN** records are inserted, updated, expired, or removed before the next
  page
- **THEN** traversal continues strictly after the prior key up to the original
  terminal key without restart or snapshot state

#### Scenario: Insertions sort after the initial terminal

- **WHEN** new records sort after the terminal key captured on the first page
- **THEN** they cannot extend that traversal and require a later lookup

#### Scenario: Hostile response does not advance semantically

- **WHEN** a responder returns duplicate/unordered/out-of-window items, changes
  the terminal, or advances a cursor on an empty page
- **THEN** the requester rejects the page without adding candidates or
  continuing from that cursor

### Requirement: Role boundaries and version features are validated

A persistent participant SHALL advertise exactly
`("swarm-ad-store", 1)`, maintain durable bounded state, and relay ads. A node
without it SHALL be treated as transient. The feature MUST NOT grant privilege.
Local and remote version messages SHALL allow at most 10 external addresses and
10 features; node ID SHALL be at most 64 encoded bytes, app name at most 32,
semver prerelease and build strings at most 32 each, every URL at most 1,024,
and every feature name at most 32. Complete outgoing `VersionMessage` and
`VerackMessage` encoded-size validation MUST succeed before send. Inbound
decoding of both messages MUST check every declared variable length/count,
including semver strings, before reservation or allocation while preserving
existing field order and valid wire encoding byte-for-byte.

A transient SHALL accept no inbound overlay connections, author no ads, and
persist no swarm-overlay ad/query/history state. It MAY keep bounded in-memory
ads and the bounded successful-endpoint-only cache. Swarm lifecycle persistence and
transport-managed state are separate scopes and MUST be documented separately.
Both roles apply identical decoding, validation, authorization, and work
bounds.

#### Scenario: Claimed role grants no privilege

- **WHEN** a malicious peer self-declares the persistent feature
- **THEN** it receives no validation, query, metering, or storage exemption

#### Scenario: Complete version message is oversized

- **WHEN** otherwise valid local fields combine into an oversized outgoing
  version message
- **THEN** it is rejected before send

#### Scenario: Remote feature count is oversized

- **WHEN** an inbound payload declares more than 10 features despite fitting
  the total payload bound
- **THEN** decoding rejects it before reserving the declared vector capacity

#### Scenario: Remote semver string is oversized

- **WHEN** inbound version or verack declares an overlong prerelease/build
  string within the total payload bound
- **THEN** decoding rejects it before allocating the declared string

#### Scenario: Complete verack is oversized

- **WHEN** local app/version fields would exceed `VERACK_MAX_BYTES`
- **THEN** verack is rejected before send

#### Scenario: Transient overlay persistence

- **WHEN** a transient disconnects
- **THEN** swarm-overlay state retained by the module is at most its bounded
  successful connect-URL/resolved-endpoint cache, while separately configured
  swarm/transport state follows its own documented policy

### Requirement: Resource accounting covers amplification paths

Every message type SHALL have hard per-channel metering. In a 10-second window,
one channel SHALL accept at most 32 ads, 16 direct lookup requests, 16 direct
responses, 4 public-list requests, 4 public-list responses, and 16 bounded
errors before strict-policy delay/penalty. It SHALL also receive at most 32
store-write and 32 relay-enqueue work tokens per 10 seconds and at most
1,048,576 response bytes per 60 seconds.

Variable fields SHALL be validated before allocation or work. Configured local
limits SHALL use these defaults and MUST NOT exceed these maxima:

| Resource | Default | Maximum |
|---|---:|---:|
| relay queue jobs | 1,024 | 4,096 |
| concurrent durable writes | 8 | 32 |
| concurrent query reads | 16 | 64 |
| concurrent relay workers | 8 | 32 |
| pages consumed per direct join lookup | 16 | 16 |
| pages consumed per public enumeration | 4 | 16 |
| candidate addresses per join attempt | 64 | 256 |
| previously compatible retry attempts | 16 | 64 |
| persisted compatible retry URLs per swarm | 64 | 256 |
| local-author reserve swarm partitions | 32 | 256 |
| active swarms | 32 | 256 |
| concurrent lifecycle attempts | 8 | 32 |
| shutdown deadline seconds | 120 | 600 |
| pending-request timeout seconds | 10 | 60 |
| configured ordinary overlay peers | 8 | 256 |
| overlay bind/listener addresses | 1 | 16 |
| serving bind addresses per swarm | 1 | 16 |
| serving external addresses per swarm | 1 | 32 |
| overlay inbound channels | 64 | 256 |
| overlay outbound channels | 8 | 64 |
| overlay manual channels | 8 | 256 |
| total established overlay channels | 80 | 512 |
| untrusted dial concurrency | 4 | 16 |
| untrusted dial starts per minute | 32 | 128 |
| DNS resolutions per join attempt | 64 | 256 |
| dials per resolved destination per attempt | 1 | 1 |

Overlay instances SHALL use strict ban policy. Queue/concurrency/retry settings
outside these ranges SHALL be rejected at configuration time.

A small request MUST NOT induce an unbounded response, write, relay, allocation,
or outbound connection. Budget accounting SHALL use ephemeral channel IDs, not
peer addresses, and SHALL be removed on disconnect.

#### Scenario: Query amplification is bounded

- **WHEN** a channel floods minimal valid requests
- **THEN** pending state, response bytes, and processing stay bounded and strict
  penalties apply

#### Scenario: Advertisement cannot induce dialing

- **WHEN** an ad contains an attacker-selected shareable address
- **THEN** accepting, storing, relaying, expiring, or reporting it opens no
  connection to that address

### Requirement: Swarm joining alone validates advertised addresses

Lookup results SHALL remain ephemeral typed original-URL/resolved-socket targets
for the requested swarm until compatibility succeeds; they MUST NOT enter a
URL-only host/refinery set first. A joining swarm MUST apply ordinary magic,
application-name, and major/minor checks before treating a peer as compatible.
Failure MUST drop the candidate, remain fallible, and MUST NOT penalize the
overlay relay. Passing compatibility does not authenticate an operator or
authorize application access. Future ordinary reconnect/refinement attempts
MUST independently resolve and validate a new exact target.

Before resolution or dialing, the joiner SHALL place previously compatibility-
verified persisted ordinary peers in one tier and fresh overlay URLs in another.
The swarm retry index itself SHALL contain at most the configured persisted-
compatible cap; when full, a newly compatible URL remains usable for its current
session but MUST NOT evict an existing retry URL merely to enter that index.

A direct join lookup SHALL continue through the first page's terminal until no
next cursor remains or its fixed 16-page cap is consumed. It SHALL use bounded
`OsRng` reservoir sampling across every valid URL returned in that traversal to
select at most the candidate-address cap, rather than truncating an ordered
prefix when candidate capacity is reached. The full bounded persisted index and fresh
reservoir SHALL then be independently shuffled with `OsRng`; wire, store, URL,
hash, DNS-answer, or lexical order MUST NOT choose the attempted prefix.

After both URL tiers are built, candidate preparation SHALL consume no more than
half the then-remaining overall deadline; verified and fresh resolution/
validation SHALL each have half that subdeadline. Unused verified time MAY be
donated to fresh preparation but not conversely. Every persisted peer MUST
still undergo fresh resolution and egress validation.

Already validated targets SHALL be installed as a two-phase pre-start manual
plan. At swarm start, the remaining candidate-dial duration SHALL be split at a
monotonic midpoint. The verified phase MUST stop/cancel by that midpoint and
consume no more than its configured retry limit or half the total candidate-
attempt budget. Fresh targets activate for the second half and retain at least
half the attempt capacity; if the verified phase is empty, fresh dialing MAY
begin immediately. Every connector uses the exact validated target. No fresh
candidate is persisted before compatibility.

#### Scenario: Wrong swarm is rejected

- **WHEN** an advertised peer fails a bound compatibility field
- **THEN** it does not enter the verified ordinary swarm peer set

#### Scenario: Relay is not blamed

- **WHEN** a relayed address fails swarm connection
- **THEN** the immediate overlay relay is not treated as author

#### Scenario: Attacker grinds lexical address order

- **WHEN** an overlay response contains many addresses chosen to sort before an
  honest candidate
- **THEN** the client shuffles the complete bounded fresh tier with `OsRng`
  before resolution/dial selection, so lexical order does not choose the budget

#### Scenario: Previously compatible tier is large

- **WHEN** persisted compatible peers exceed their retry limit
- **THEN** the bounded full index is shuffled, a subset consumes at most half
  the attempt budget and first half of dial time, and fresh overlay candidates
  retain the remainder

#### Scenario: Ordered lookup exceeds candidate capacity

- **WHEN** terminal-bounded lookup returns more URLs than the candidate cap
- **THEN** bounded CSPRNG reservoir sampling covers every URL returned through
  terminal completion or the 16-page cap instead of taking its lexical prefix

### Requirement: Metadata disclosure and identity scoping are explicit

Wire messages MUST NOT intentionally bind different swarms to one stable node
or signing identity. Durable state MUST NOT contain querier identity or query
history. An answering peer nevertheless observes the requested ID; requests on
one channel are linkable; timing, topology, public enumeration, and endpoint
reuse are metadata surfaces. The capability MUST NOT claim PIR, guaranteed
origin anonymity, absence of remote traces, or global-observer resistance.

Gossip and store peers necessarily observe and MAY retain every swarm-ID to
advertised-endpoint mapping they receive. The forbidden provenance association
is a mapping from overlay source peer to swarm/ad authorship; the rendezvous
ID-to-endpoint mapping is intentional protocol output and is not confidential.

Every overlay and swarm `P2p` instance SHALL use an independently CSPRNG-
generated `VersionMessage.node_id`; it MUST NOT be persisted or reused across
instances, swarms, overlay/swarm roles, or process restart.

Serving documentation SHALL identify endpoint reuse as directly linkable and
SHALL NOT claim automatic independent Tor/I2P provisioning.

#### Scenario: Query disclosure is documented

- **WHEN** a lookup for S is issued
- **THEN** documentation states the answering peer observes S

#### Scenario: Overlay and swarm version identities

- **WHEN** one process starts an overlay and one or more swarm `P2p` instances
- **THEN** their version node IDs are independently generated and unequal

#### Scenario: Shared endpoint is linkable

- **WHEN** one endpoint is advertised for two swarms
- **THEN** guidance identifies the direct link and makes no contrary claim
