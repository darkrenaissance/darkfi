## Purpose

Defines how the event graph carries application-defined payloads: a content
type tag byte that separates RLN payloads from application payloads in
the static DAG, and a leading tag byte that discriminates darkirc rotating-DAG
content types without trial deserialization, while leaving RLN semantics
unchanged.

## ADDED Requirements

### Requirement: Static-DAG content discrimination by tag byte

Static-DAG event content SHALL be discriminated by its first byte. Content
whose encoding begins with an RLN payload (first byte `0x00` or `0x01`,
matching the two RLN node variant encodings) MUST be processed by the existing
RLN admission pipeline with unchanged semantics. Any other first byte identifies
an application payload.

#### Scenario: RLN registration still admitted
- **WHEN** a node receives a static event containing an RLN registration or
  slash in the existing encoding
- **THEN** the event is verified, committed, and relayed exactly as before this
  change, and the RLN identity state is updated accordingly

#### Scenario: Application payload never touches RLN state
- **WHEN** a node receives a static event whose content tag identifies an
  application payload
- **THEN** the event is admitted or rejected by application-payload rules and
  the RLN identity tree, identity state, and historical roots are not modified

### Requirement: Application payload admission to the static DAG

A static event carrying an application payload SHALL be admitted when it passes
the existing structural validation for static events (non-empty content,
content hash matching the header, well-formed parents, parents present in the
static DAG) and its content length is within a defined bound. Admission MUST
NOT depend on RLN being enabled or disabled: both modes apply the same
structural rules. Semantic validity (e.g. signatures, chain links) is resolved
by the consuming application after admission, not at admission time.

#### Scenario: Admitted on an RLN-enabled node
- **WHEN** a structurally valid, correctly tagged application payload arrives
  at a node with RLN enabled
- **THEN** the event is stored in the static DAG and relayed to peers

#### Scenario: Admitted on an RLN-disabled node
- **WHEN** the same payload arrives at a node with RLN disabled
- **THEN** the event is stored and relayed under the same structural rules

#### Scenario: Oversized payload rejected
- **WHEN** an application payload exceeds the defined content bound
- **THEN** the event is rejected and not relayed

### Requirement: RLN state rebuild skips application payloads

Any rebuild or audit of RLN state from persisted static-DAG events MUST skip
application payloads deterministically, producing the same identity tree as a
node that never saw them.

#### Scenario: Rebuild after mixed history
- **WHEN** a node rebuilds RLN state from a static DAG containing both RLN
  events and application payloads
- **THEN** the resulting identity tree and historical roots are identical to a
  rebuild from the RLN events alone

### Requirement: Unknown or malformed content is skipped without penalty

Content that a node cannot interpret — an unknown rotating tag, or an
application payload that fails structural parsing — MUST be skipped without
striking, banning, or crashing, and without affecting other events in the
same batch or relay path.

#### Scenario: Unknown rotating tag skipped
- **WHEN** a rotating event's first byte is a tag the client does not know
- **THEN** the event is skipped and the peer suffers no penalty

#### Scenario: Malformed application static payload skipped
- **WHEN** an app-tagged static event fails structural parsing
- **THEN** the event is skipped and the peer suffers no penalty

### Requirement: Rotating-DAG darkirc content tag byte

All darkirc content in rotating DAGs SHALL begin with a tag byte identifying
the content type; no untagged form exists. The tag set MUST include Privmsg
(with optional signer key and signature fields) and hide action. Relay paths
MUST dispatch on the tag byte instead of attempting deserialization of each
known type in turn.

#### Scenario: Dispatch by tag
- **WHEN** a client relays a rotating event whose first byte is a known tag
- **THEN** the content is decoded as the type named by the tag without
  attempting other decodings

#### Scenario: Unknown tag skipped
- **WHEN** a rotating event's first byte is a tag the client does not know
- **THEN** the event is skipped without error propagation

### Requirement: Tag byte must not leak encryption target

The tag byte MUST NOT distinguish an encrypted channel message from an
encrypted direct message. Identifying the encryption target of an encrypted
payload remains a local decryption attempt, so observers of the DAG learn only
the content type, never the message category.

#### Scenario: Encrypted payloads share one tag
- **WHEN** two encrypted messages are published, one to a channel and one as a
  direct message
- **THEN** their event content begins with the same tag byte and an observer
  cannot distinguish their category from the DAG
