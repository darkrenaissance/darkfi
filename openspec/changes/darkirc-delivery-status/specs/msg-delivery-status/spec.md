# Spec Delta: msg-delivery-status

## Purpose

Gives senders of event-graph broadcast messages evidence about whether
peers accepted them, and defines the sender-side rebroadcast/recreate
policy that prevents messages written while offline from being silently
lost to DAG rotation. Covers the reply wire format, delivery-state
exposure to applications, darkirc's outbound tracking and recreate flow,
and client-side dedup/display of delivery state.

## ADDED Requirements

### Requirement: EventPut outcome reply

A node that receives an `EventPut` SHALL send the sender an
`EventPutStatus` reply on the same channel describing the outcome:

- `Has { inserted: true }` when the event was newly accepted into the
  node's DAG
- `Has { inserted: false }` when the node already has the event
- `Nack { reason: TooOld }` when the event predates the node's current
  rotation window
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
- **THEN** it replies `Nack { reason: TooOld }`

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
outbound record containing the event id, the message's `uid`, the
plaintext message fields needed to rebuild it, a state, and an attempt
counter. Records SHALL survive restart. Records SHALL be closed
(removable) once a positive outcome is observed or the attempt cap is
reached.

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
  earlier propagation
- **THEN** their `Has { inserted: false }` replies close the record and
  no recreation happens

### Requirement: Recreate on unrecoverable or rejected delivery

darkirc SHALL create a replacement event from the stored plaintext —
fresh timestamp and DAG parents, fresh encryption nonce, same `uid` —
when any of:

- a `Nack { reason: TooOld }` arrives (the rotation window has passed)
- every observed reply across the retry rounds is a nack
- zero replies are observed after the maximum number of rebroadcast
  rounds

Recreation SHALL consume a fresh rate-limit slot when rate limiting is
enabled; if the epoch budget is exhausted the attempt SHALL be parked
and retried after epoch rollover rather than dropped or sent unproven.
Recreation attempts SHALL be capped; once the cap is reached the record
SHALL be closed as failed and the failure surfaced to the client.

#### Scenario: Rotation makes the original undeliverable

- **WHEN** a pending record's event predates the current rotation window
  and any peer nacks `TooOld`
- **THEN** a replacement event with the same `uid` and fresh
  parents/timestamp is published

#### Scenario: Budget exhaustion parks, not drops

- **WHEN** recreation is due but the rate-limit epoch budget is spent
- **THEN** no unproven replacement is broadcast and the record waits for
  the next epoch

#### Scenario: Attempt cap surfaces failure

- **WHEN** the configured recreation attempt cap is reached without any
  positive outcome
- **THEN** the record is closed as failed and the client is informed

### Requirement: Message uid for recreate dedup

The chat message payload SHALL carry a sender-generated `uid` that
remains identical across recreations of the same message. Receiving
clients SHALL treat messages with equal `uid` as one logical message for
display purposes. Payload decoding SHALL be version-aware so peers that
do not understand the new field still decode old payloads.

#### Scenario: Recreated message does not double-render

- **WHEN** a receiving client has already displayed the original message
  and later receives its recreation
- **THEN** the recreation is not rendered as a second message

#### Scenario: Old payloads still decode

- **WHEN** a client that understands `uid` receives a payload without
  one
- **THEN** the payload decodes via its version and renders normally

### Requirement: App displays delivery state

The app SHALL subscribe to delivery statuses for its own outbound
messages and expose per-message delivery state keyed by `uid`:
`sending` (no replies yet), `delivered` (any `Has` reply), `failed`
(record closed at the attempt cap). Delivery state SHALL NOT be
presented as evidence that any particular recipient read the message.

#### Scenario: Tick on first acceptance

- **WHEN** any peer replies `Has` for the app's outbound message
- **THEN** the message's state becomes `delivered`

#### Scenario: Failure is visible

- **WHEN** a message's record closes as failed
- **THEN** the app shows the message as failed rather than leaving it
  indefinitely in `sending`
