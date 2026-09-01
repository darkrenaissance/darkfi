## Purpose

Guarantees that a long-lived event-graph client repairs missed rotating-DAG
events after a connectivity interruption (device sleep, network churn, or a
slot rotation) instead of silently accumulating permanent history gaps, and
makes such repairs observable in logs.

## ADDED Requirements

### Requirement: Resume triggers a repair round

When connectivity is re-established after an interruption, the client SHALL
schedule a repair round for the current rotation slot. Triggers SHALL include
at minimum: an explicit wake signal (screen-on / start), and a transition of
connected outbound peers from zero to one or more.

#### Scenario: Resume after suspension

- **WHEN** the client's outbound connections were suspended (e.g. screen off)
  and are reactivated while peers hold events the client does not have
- **THEN** the client performs a repair round for the current slot and commits
  the previously-missing events to its DAG

#### Scenario: Peer recovery without wake signal

- **WHEN** the client had zero connected peers and at least one peer connects
- **THEN** a repair round for the current slot is scheduled

#### Scenario: OS-level suspend on a machine with no wake signal

- **WHEN** a desktop/laptop client's process is frozen by OS suspend (e.g. lid
  close) and later resumed, so that peers reconnect without any in-app wake
  signal firing
- **THEN** a repair round for the current slot is still scheduled via peer
  recovery or the periodic interval, since clients MUST NOT assume the machine
  stays running and connected for the duration of the session

### Requirement: Repair rounds run periodically

The client SHALL re-run a repair round for the current rotation slot on a
fixed interval, so that failed rounds are retried, slots that rotate in while
the client is long-lived are reconciled, and gaps are repaired even when no
wake or connection event is observed.

#### Scenario: Rotated-in slot is reconciled

- **WHEN** a new rotation slot becomes current and the periodic interval
  elapses
- **THEN** the client runs a repair round against the new slot

### Requirement: Repair fetches events unreachable from local tips

A repair round SHALL fetch headers for events that are not ancestors of the
client's current tips (i.e. events on unreached branches), not only events
newer than the client's tips, and SHALL fetch corresponding event bodies for
headers it newly learns. A repair round SHALL NOT be skipped merely because
the client already holds the current network tips.

#### Scenario: Mid-history gap repaired after tips are current

- **WHEN** the client already holds all current network tips but is missing
  events on branches unreached by gossip ancestry walks, and a repair round
  runs
- **THEN** the missing branch events are fetched and committed

#### Scenario: Events only servable by a minority of peers

- **WHEN** a missing event is held by at least one connected, serving peer
- **THEN** the repair round is able to fetch it (repair MUST NOT require a
  quorum of peers to hold the event)

### Requirement: Repair round failure handling

A repair round SHALL be lenient toward per-event failures: events that cannot
be committed (e.g. a serving peer lacks the event's required proof blob)
SHALL be skipped with a log entry rather than aborting the entire round.
Skipped events remain eligible for later repair rounds.

#### Scenario: Peer lacks a blob for one event

- **WHEN** a repair round fetches a batch of bodies and one event is unservable
- **THEN** the other events in the round are committed, the unservable one is
  logged, and the round is not reported as failed

### Requirement: Repair does not disrupt initial sync or live ingestion

A repair round SHALL NOT run concurrently with an initial DAG sync or with
another repair round for the same slot. Repair SHALL NOT clear or toggle the
synced state that gates live event ingestion, and SHALL NOT run before
initial sync has completed.

#### Scenario: Trigger while initial sync is in flight

- **WHEN** a repair trigger fires while the initial sync is still running
- **THEN** the repair round is deferred until the initial sync completes

### Requirement: Repair observability

Each repair round SHALL emit a log entry when it starts (including the slot
and trigger source) and when it completes (including counts of headers
gained, event bodies committed, and events skipped with reasons).

#### Scenario: Successful repair is visible in logs

- **WHEN** a repair round commits previously-missing events
- **THEN** an operator inspecting logs can determine the trigger, the slot,
  and how many events were repaired
