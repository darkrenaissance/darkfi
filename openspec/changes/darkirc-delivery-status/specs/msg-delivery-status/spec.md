# Spec Delta: msg-delivery-status

## Purpose

Gives senders of event-graph broadcast messages evidence about whether
peers accepted them, and defines a durable, oldest-first broadcast queue
that retains messages until acknowledged rather than silently losing
them to DAG rotation. Covers the reply wire format, delivery-state
exposure to applications, and darkirc's head-only retry/recreate flow as
the reference implementation. ACKs are unverified network-delivery
evidence, not recipient receipts or a guarantee of recipient display order.

## ADDED Requirements

### Requirement: EventPut outcome reply

A node that receives an `EventPut` SHALL send the sender an
`EventPutStatus` reply on the same channel describing the outcome:

- `Has { inserted: true }` when the event was newly accepted into the
  node's DAG
- `Has { inserted: false }` when the node already has the event,
  **including when the event is only found in a retained older rotation
  slot** — the receiver SHALL check its retained slots before nacking
  `TooOld`, so a peer still holding a rotated-away message answers
  `Has` rather than `TooOld`
- `Nack { reason: TooOld }` when the event predates the node's current
  rotation window and is not present in any retained slot
- `Nack { reason: NotSynced }` when the node is still performing its
  initial DAG sync and skipped the event
- `Nack { reason: Invalid }` when the event fails structural or
  proof validation
- `Nack { reason: Busy }` when a transient parent-fetch failure or an
  internal condition prevented processing, without established invalidity

Replies apply to relayed events equally: the receiver cannot and does not
distinguish originator from relay.

#### Scenario: Fresh event accepted

- **WHEN** a peer receives an `EventPut` for an event it does not have,
  which passes all validation
- **THEN** it replies `Has { inserted: true }`

#### Scenario: Event already known

- **WHEN** a peer receives an `EventPut` whose event is already in its
  DAG
- **THEN** it replies `Has { inserted: false }`

#### Scenario: Event older than the rotation window

- **WHEN** a peer receives an `EventPut` whose event timestamp precedes
  its current genesis
- **THEN** it replies `Nack { reason: TooOld }` only if the event is not
  present in any of its retained rotation slots

#### Scenario: Event held in a retained older rotation slot

- **WHEN** a peer receives an `EventPut` for an event it holds in a
  retained older rotation slot (for example after the sender
  disconnected and the DAG rotated)
- **THEN** it replies `Has { inserted: false }` rather than
  `Nack { reason: TooOld }`

#### Scenario: Receiving node still syncing

- **WHEN** a peer that has not finished initial sync receives an
  `EventPut`
- **THEN** it replies `Nack { reason: NotSynced }`

#### Scenario: Transient processing failure is not invalidity

- **WHEN** parent retrieval fails transiently without establishing that
  the event is invalid
- **THEN** the receiver replies `Nack { reason: Busy }`, not `Invalid`

#### Scenario: Malicious input keeps existing policing

- **WHEN** a peer receives an `EventPut` that trips existing
  strike/flood/parent-depth policing
- **THEN** existing strike, ban, and disconnect behavior is unchanged
  and no status reply is owed

### Requirement: Coarse nack reasons and stable wire format

`EventPutStatus` payloads SHALL carry only the event id and a coarse
outcome. Nack reasons SHALL NOT encode fine-grained validation detail.
All enum variants SHALL use explicit `u8` discriminants so the wire
format is stable. Decoding an `EventPutStatus` with an unknown variant
or malformed body SHALL be rejected without panicking, and the receiving
node SHALL NOT strike or ban the sender solely for an undecodable
status.

#### Scenario: Unknown variant from a future peer

- **WHEN** a node decodes an `EventPutStatus` carrying a variant it does
  not know
- **THEN** the status is discarded, the connection stays up, and no
  panic occurs

#### Scenario: Statuses carry no message content

- **WHEN** any `EventPutStatus` is serialized
- **THEN** it contains only the event id and outcome — no channel, nick,
  plaintext, or proof material

### Requirement: Delivery-state exposure to applications

The event graph SHALL expose a subscription that republishes every
inbound `EventPutStatus` unfiltered, together with the peer channel it
arrived on. The event graph itself SHALL NOT track outbound deliveries
or make retry decisions; that policy belongs to applications.

#### Scenario: Application observes replies for its own messages

- **WHEN** a reply arrives for an event an application previously
  broadcast
- **THEN** the application's subscription delivers the reply and the
  application can correlate it with the event id

#### Scenario: Replies for relayed events are also published

- **WHEN** a reply arrives for an event the node relayed (not
  originated)
- **THEN** the subscription still delivers it; consuming it is the
  application's choice

### Requirement: Durable oldest-first broadcast queue

darkirc SHALL durably enqueue each accepted outgoing chat message in one
global FIFO, preserving its plaintext rebuild fields and enqueue order
across restart. Only the oldest unacknowledged logical message, the head,
SHALL be eligible for event creation or network exposure. Later messages
SHALL remain queued without publishing events, including through DAG
relay/sync. The head SHALL be materialized and sent only when darkirc is
DAG-synced and has the configured minimum number of peer connections.
FIFO guarantees local send sequencing, not recipient display ordering.

#### Scenario: Later messages cannot overtake the head

- **WHEN** messages A and B are accepted in that order and A is not ACK'd
- **THEN** B remains queued without an event being created or exposed to
  peers, even while A is disconnected, retrying, or parked

#### Scenario: Offline enqueue order survives restart

- **WHEN** messages are accepted while offline or initially unsynced and
  darkirc restarts before sending them
- **THEN** their contents and queue order survive, and only the oldest
  becomes eligible when sync/connectivity requirements are met

### Requirement: Recoverable head generations before network exposure

Before exposing a head generation to the network, darkirc SHALL durably
retain its exact event and blob. Unchanged retries SHALL remain possible
after local DAG pruning. Replacements SHALL keep the logical message's
queue position and preserve old-to-new supersession links and generation
ids until ACK. Committing a replacement SHALL be crash-recoverable, with
only one active generation; restart SHALL NOT bypass the head or generate
a second replacement for an already committed transition.

#### Scenario: Pruning does not prevent unchanged rebroadcast

- **WHEN** a head event is pruned from all local DAG slots before ACK and
  darkirc restarts
- **THEN** its exact event and blob remain available for rebroadcast

#### Scenario: Replacement persistence fails

- **WHEN** a replacement cannot be durably retained
- **THEN** it is not exposed to peers and the existing head remains queued

#### Scenario: Crash after replacement commit

- **WHEN** darkirc restarts after committing a replacement but before
  broadcasting it
- **THEN** it resumes that generation at the same queue position, with
  the old generation's supersession link intact

### Requirement: ACK-only queue advancement

A `Has` outcome with either inserted value for any generation of the head
SHALL ACK its logical message. An accepted foreign event whose ancestry
includes any such generation SHALL also ACK it, using local computation
over received events without additional wire queries. Locally generated
events alone SHALL NOT constitute foreign delivery evidence.

ACK SHALL durably dequeue the logical message exactly once, cancel its
pending retries/recreation, and permit service of the next head. ACK
processing SHALL NOT depend on retry timers or the current connection
count. Duplicate, unrelated, or late statuses SHALL NOT remove another
message. Concurrent recreation work SHALL NOT resurrect an ACK'd entry.
An ACK is unverified network evidence, not proof of forwarding or recipient
receipt; already-broadcast generations cannot be recalled.

#### Scenario: Either Has variant advances the queue

- **WHEN** the head receives `Has { inserted: true }` or
  `Has { inserted: false }` for one of its generations
- **THEN** it is durably dequeued and the next message becomes eligible

#### Scenario: Foreign descendant acknowledges the head

- **WHEN** an accepted foreign event's ancestry contains a head generation
- **THEN** the head is ACK'd without waiting for a status reply

#### Scenario: Own events are not ACKs

- **WHEN** only a locally generated event references the head generation
- **THEN** that reference does not advance the queue

#### Scenario: Old-generation ACK races with recreation

- **WHEN** an ACK for an older head generation arrives during recreation
- **THEN** the logical message is dequeued exactly once and stale work
  cannot restore it or start further retries

#### Scenario: Duplicate ACK after restart

- **WHEN** an ACK'd head has been durably dequeued, darkirc restarts, and
  another ACK arrives for that removed message
- **THEN** the current head remains queued unless independently ACK'd

### Requirement: Retry the head until ACK

For a materialized, unacknowledged head, darkirc SHALL rebroadcast the
active event unchanged when sync/connectivity requirements are met. It
SHALL use backoff for a configured number of rounds and continue at a
capped slow rate thereafter. There SHALL be no total retry-round,
recreation-attempt, or lifetime limit that discards an unacknowledged
message or advances the queue. Negative replies SHALL NOT bypass backoff.

#### Scenario: Reconnect retries only the head

- **WHEN** queued messages exist and sync/connectivity requirements are
  restored for a materialized head
- **THEN** the head's active event is rebroadcast unchanged and later
  messages are not sent

#### Scenario: Retry limits never discard the head

- **WHEN** repeated retry rounds and rotations pass without ACK
- **THEN** the logical message remains at the head; retries and eligible
  recreations continue at bounded rate while required resources exist

### Requirement: Recreate the head only after expiry

darkirc SHALL recreate the active head event only when its rotation
window has closed, known locally or indicated by `TooOld`, and a
rebroadcast round completes without ACK for any head generation. This
SHALL apply even when no status replies have ever arrived. The replacement
SHALL use fresh timestamp, parents, and encryption nonce, preserve queue
position, and link the old generation to the new one locally.

Silence, `Invalid`, `NotSynced`, and `Busy` alone SHALL NOT cause recreation
or dequeue. Any ACK SHALL stop further recreation of that logical message.

#### Scenario: Local expiry with silent peers

- **WHEN** the head's active event is locally known to be expired and a
  rebroadcast round completes without ACK, even with no replies ever
  received, and materialization resources are available
- **THEN** a replacement is durably retained and sent at the same queue
  position; no later message is sent

#### Scenario: Retained-slot holder answers before recreation

- **WHEN** an expired head receives `Has { inserted: false }` from a peer
  holding it in a retained slot
- **THEN** it is ACK'd and dequeued without recreation

#### Scenario: Rejection or silence during an open window

- **WHEN** the active window is open and the head sees only silence,
  `Invalid`, `NotSynced`, or `Busy`
- **THEN** it retries unchanged with backoff, without recreation or dequeue

### Requirement: Park rather than discard a blocked head

Every new generation SHALL use a fresh rate-limit slot when rate limiting
is enabled. Exhausted budget SHALL park the head until epoch rollover;
missing identity SHALL park it until identity is available. Processing
or storage failures SHALL preserve the head and prevent later sends from
bypassing it. Retrying the same event SHALL NOT re-encrypt it or consume
a new rate-limit slot. No unproven replacement SHALL be sent. Retryable
errors SHALL be retried at bounded rate, not treated as terminal dequeue.
Blocked-send error notices to connected clients SHALL NOT imply removal
from the queue; receipt/status display remains out of scope.

#### Scenario: Budget exhaustion parks the whole queue

- **WHEN** the head needs a new generation but the rate-limit epoch budget
  is spent
- **THEN** the head waits for rollover without publishing an unproven
  event, and later messages remain queued

#### Scenario: Missing identity does not lose the head

- **WHEN** an enabled rate-limit path cannot obtain an identity
- **THEN** the head remains queued and can resume when identity becomes
  available, without advancing to another message

### Requirement: Bounded queue storage with explicit backpressure

Queue storage SHALL have a configured bound covering pending messages,
active event/blob data, and retained generation metadata. When a new
enqueue cannot fit or cannot be persisted, darkirc SHALL explicitly reject
it to the connected client rather than report acceptance. Storage pressure
SHALL NOT evict unacknowledged entries or their required retry/correlation
data. Recreation that cannot fit SHALL park the head. ACK'd entries MAY
be removed to reclaim queue storage.

#### Scenario: Full queue rejects a new send

- **WHEN** a new message would exceed the queue storage bound
- **THEN** its enqueue is explicitly rejected and existing queued messages
  retain their contents and order

#### Scenario: Generation metadata fills the storage budget

- **WHEN** recreation would exceed the bound for retained generation data
- **THEN** the head parks without dropping previous generation ids or
  allowing later messages to bypass it
