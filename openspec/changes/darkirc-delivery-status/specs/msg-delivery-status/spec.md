# Spec Delta: msg-delivery-status

## Purpose

Gives senders of event-graph broadcast messages evidence about whether
peers accepted them, and defines the sender-side rebroadcast/recreate
policy that prevents messages written while offline from being silently
lost to DAG rotation. Covers the reply wire format, delivery-state
exposure to applications, and darkirc's outbound tracking, rebroadcast,
and recreate flow as the reference implementation.

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
- `Nack { reason: Busy }` when an internal condition prevented
  processing

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

### Requirement: darkirc persists outbound records before broadcast

When darkirc publishes a chat message event, it SHALL first persist an
outbound record containing the event id, the plaintext message fields
needed to rebuild it, a state, an attempt counter, and a link to any
superseding replacement event. Records SHALL survive restart. Records
SHALL be closed (removable) once a positive outcome is observed or the
attempt cap is reached.

#### Scenario: Restart does not lose pending sends

- **WHEN** darkirc commits an outbound event locally, broadcasts it, and
  the process restarts before any reply arrives
- **THEN** after restart the outbound record is still present and the
  delivery policy resumes evaluating it

### Requirement: Rebroadcast-first delivery policy

For an outbound record with no positive outcome, darkirc SHALL wait
until it is DAG-synced and has at least a configured minimum number of
peer connections, then rebroadcast the original event unchanged. It
SHALL retry with backoff, bounded by a configured maximum number of
rounds. A single `Has` outcome (either `inserted` value) SHALL close the
record as delivered.

#### Scenario: Reconnect triggers rebroadcast

- **WHEN** a pending record exists, the node is synced, and the minimum
  connection count is reached
- **THEN** the original event is rebroadcast unchanged

#### Scenario: Prior propagation is detected, not recreated

- **WHEN** a rebroadcast reaches peers that already hold the event via
  earlier propagation or in a retained older rotation slot
- **THEN** their `Has { inserted: false }` replies close the record and
  no recreation happens

### Requirement: Ancestry reference counts as delivery evidence

The delivery monitor SHALL treat a foreign event whose DAG ancestry
includes one of its tracked outbound events as positive delivery
evidence, closing the outbound record as delivered. This is a local
computation over already-received events and adds no wire traffic.

#### Scenario: Foreign child closes the record

- **WHEN** a new event arrives whose ancestry (walked through parent
  references) contains a pending outbound event
- **THEN** the outbound record is closed as delivered without waiting
  for any explicit status reply

### Requirement: Recreate only on explicit evidence

darkirc SHALL create a replacement event from the stored plaintext —
fresh timestamp and DAG parents, fresh encryption nonce — only when
either:

- the rotation window for the original event is closed (known locally
  from the rotation schedule, or indicated by `TooOld` nacks) and no
  peer has answered `Has` for it, or
- every observed reply is a nack and at least one carries reason
  `Invalid`

The following observations SHALL NOT trigger recreation while the
rotation window is open: silence, `Nack { NotSynced }`, and
`Nack { Busy }` mean retry later. Silence SHALL NOT trigger recreation
even after the window closes when no status reply of any kind has ever
been received from status-capable peers. A replacement links back via
the supersession chain so the sender can correlate attempts locally.

Recreation SHALL consume a fresh rate-limit slot when rate limiting is
enabled; if the epoch budget is exhausted the attempt SHALL be parked
and retried after epoch rollover rather than dropped or sent unproven.
Recreation attempts SHALL be capped; once the cap is reached the record
SHALL be closed as failed and the failure surfaced to the client.

#### Scenario: Rotation makes the original undeliverable

- **WHEN** a pending record's rotation window has closed and no peer
  answers `Has` after a rebroadcast round
- **THEN** a replacement event with fresh parents/timestamp is published
  and linked via the supersession chain

#### Scenario: Holder answers before recreation

- **WHEN** the rotation window has closed but a rebroadcast round
  returns `Has { inserted: false }` from any peer holding the event in a
  retained slot
- **THEN** the record is closed as delivered and no replacement is
  created

#### Scenario: Silence never recreates on its own

- **WHEN** no status replies of any kind are observed (for example all
  peers run versions without status support)
- **THEN** the monitor keeps rebroadcasting at a bounded rate and does
  not create replacements

#### Scenario: Transient nacks mean retry

- **WHEN** observed replies are `Nack { NotSynced }` or `Nack { Busy }`
  while the rotation window is open
- **THEN** the monitor retries later and does not recreate

#### Scenario: Explicit rejection recreates

- **WHEN** every observed reply across the retry rounds is a nack and at
  least one carries reason `Invalid`
- **THEN** a replacement event is published (bounded by the attempt cap)

#### Scenario: Budget exhaustion parks, not drops

- **WHEN** recreation is due but the rate-limit epoch budget is spent
- **THEN** no unproven replacement is broadcast and the record waits for
  the next epoch

#### Scenario: Attempt cap surfaces failure

- **WHEN** the configured recreation attempt cap is reached without any
  positive outcome
- **THEN** the record is closed as failed and the client is informed
