## Purpose

Defines anonymous-communication-friendly, client-enforced moderation for
darkirc public channels: channel registration and owner-signed action chains
in the static DAG, a hardcoded policy model with owner-set defaults and local
user overrides, admin hide actions in the rotating DAG, and the ChanServ
command interface used by channel owners and admins.

## ADDED Requirements

### Requirement: Public channel registration

A user SHALL be able to register a public `#` channel by publishing a
registration event to the static DAG that names the channel and embeds an
owner public key. Registration is first-come-first-served: when two valid
registrations for the same name exist, every node MUST resolve the same winner
using deterministic canonical ordering of the static DAG. Only public
channels are supported in this version; channels requiring decryption
(saltbox channels and direct messages) are out of scope.

#### Scenario: Register a channel
- **WHEN** a user publishes a signed registration for a channel that has no
  valid registration
- **THEN** every synced node resolves that user's owner key as the channel
  owner

#### Scenario: Registration race resolves identically everywhere
- **WHEN** two registrations for the same channel name are published before
  either node sees the other
- **THEN** all nodes that eventually hold both events pick the same
  registration as authoritative

### Requirement: Owner action chain

Ownership-changing and policy-defining actions SHALL form a per-channel chain:
each action names the previous action's static event id and is signed by the
current owner key. Resolution MUST yield exactly one authoritative chain per
channel: the longest chain of correctly linked, correctly signed actions
starting from the winning registration, with deterministic tie-breaking. A
transfer of ownership makes subsequent actions valid only under the new key.

#### Scenario: Transfer changes signing authority
- **WHEN** an owner publishes a transfer to a new key and the new key later
  signs a policy update
- **THEN** all nodes accept the policy update and reject any further action
  signed by the old key

#### Scenario: Invalid signature cannot extend the chain
- **WHEN** an action is signed by a key that is not the current owner
- **THEN** the action is ignored during resolution and the previous chain tip
  remains authoritative

#### Scenario: Competing chains resolve deterministically
- **WHEN** two valid chains exist for one channel (e.g. after key compromise)
- **THEN** every node selects the same chain by longest-valid-chain rule with
  canonical ordering as tie-break

### Requirement: Hardcoded policy enum and default policy list

Policies SHALL be identified by a `u8` enum from a hardcoded, shared registry
(initially: posting allow-list, admin hide set, regex filter). The owner
SHALL be able to publish, as a chain action, the channel's default policy
list: for each policy, its id, opaque parameters, and a default enabled flag.
The serialized format MUST allow future policy ids and parameter shapes
without changing stored history.

#### Scenario: Publish a default policy list
- **WHEN** an owner publishes a policy list naming the admin hide policy with
  a set of admin public keys, enabled by default
- **THEN** synced nodes resolving the channel expose that policy with those
  parameters as the channel default

#### Scenario: Unknown policy id in a list
- **WHEN** a resolved policy list contains an id the client does not know
- **THEN** the client ignores that policy entry without failing resolution of
  the rest of the list

### Requirement: Admin hide actions in the rotating DAG

A key named in an enabled admin hide policy's parameters SHALL be able to
publish signed hide or unhide actions as rotating-DAG events referencing the
event id of a target message. Clients MUST mark matching messages as hidden
rather than removing them from storage; hidden state MUST be reversible in
the UI (e.g. reveal control). Resolution MUST apply the last action per
target in canonical rotating-DAG order across the retained window. Hide
actions expire with the rotation window exactly as messages do; no permanent
record of a hide is created.

#### Scenario: Hide is applied on other nodes
- **WHEN** an admin publishes a hide action for a message in a channel a user
  has joined
- **THEN** the user's client marks that message hidden and indicates that
  hidden messages exist

#### Scenario: Unhide reverses a hide
- **WHEN** a later action by an authorized key unhides the same target
- **THEN** the message is shown again for clients applying policy

#### Scenario: Unauthorized hide ignored
- **WHEN** a hide action is signed by a key not in the channel's admin hide
  set, or the admin hide policy is disabled
- **THEN** clients ignore the action

### Requirement: Posting allow-list policy

When the posting allow-list policy is enabled for a channel, clients SHALL
render only messages that carry a valid signature by a key in the policy's
parameter set. Compliant senders in such channels attach their signer public
key and signature to the message. Enforcement is client-side: unsigned
messages still propagate on the network but are hidden for clients applying
the policy.

#### Scenario: Signed message renders
- **WHEN** a message in an allow-listed channel carries a valid signature
  from an allowed key
- **THEN** clients applying the policy render it

#### Scenario: Unsigned message hidden
- **WHEN** a message in an allow-listed channel lacks a signature or is
  signed by a key outside the set
- **THEN** clients applying the policy hide it

### Requirement: Regex filter policy

When the regex filter policy is enabled, clients SHALL hide messages whose
decoded privmsg matches the filter's parameter rules. Rules are regular
expressions evaluated against the privmsg, and MAY match the nick and/or the
message content. Filter parameters are opaque to the wire format; invalid or
non-compilable rules MUST be ignored during resolution without failing
evaluation of the remaining rules.

#### Scenario: Matching message hidden
- **WHEN** a message whose nick or content matches an enabled filter's regex
  rules arrives in a joined channel
- **THEN** clients applying the policy hide it

#### Scenario: Invalid rule ignored
- **WHEN** a filter's parameters contain a rule that fails to compile
- **THEN** clients skip that rule and evaluate the remaining rules

#### Scenario: Encrypted messages filtered after decryption
- **WHEN** an encrypted message matching the rules is decrypted locally
- **THEN** the filter applies to the decoded plaintext identically to
  plaintext-channel messages

### Requirement: Local policy overrides

A user SHALL be able to override the enabled flag of any policy in a
channel's default list locally, per channel, without publishing anything.
The initial app UI exposes enable/disable of provided policies only; adding
user-defined policies is out of scope for this version.

#### Scenario: Override a default
- **WHEN** a channel's policy is enabled by owner default and the user
  disables it locally
- **THEN** that user's client applies the policy as disabled while other
  users remain unaffected

#### Scenario: Policy overlay from the channel label
- **WHEN** the user taps the channel-name label shown at the top of a
  channel's chat screen
- **THEN** the app opens an overlay for that channel listing the owner's
  current default policies with their default states, and toggling a policy
  in the overlay changes the user's local override and re-filters the
  channel view accordingly

### Requirement: Owner-signed pins with encrypted snapshots

A channel owner SHALL be able to pin a message by publishing a chain action
that embeds a snapshot of the message content and references its event id.
For a channel with a shared key, the snapshot MUST be encrypted under that
channel key so the static DAG never carries its plaintext. Pins MUST remain
renderable after the original message has rotated out of the window.

#### Scenario: Pin outlives rotation
- **WHEN** a pinned message's DAG window has expired
- **THEN** clients can still render the pinned snapshot from the static DAG

#### Scenario: Encrypted channel pin carries no plaintext
- **WHEN** an owner pins a message in a saltbox-encrypted channel
- **THEN** the static-DAG event contains only ciphertext decryptable by
  channel members

### Requirement: ChanServ command interface

The `irc2` stack SHALL provide a ChanServ service addressed by IRC private
message, mirroring the existing NickServ pattern, with commands for
registration, info, ownership transfer, policy management, pinning, and
hiding. Policy commands SHALL address policies by their registry name (e.g.
`FILTER`), parsed to the hardcoded policy id; unknown names are rejected.
Owner and admin actions MUST be authenticated by verifying the
signing key configured locally; the service MUST refuse actions for which no
matching key is configured.

#### Scenario: Register via ChanServ
- **WHEN** a user with an owner key configured sends the registration command
  for an unregistered channel
- **THEN** the registration event is signed with that key and published to
  the static DAG

#### Scenario: Policy addressed by name
- **WHEN** an owner issues a policy command using a policy's name (e.g.
  `FILTER`) and valid parameters
- **THEN** the command applies to that policy's entry in the channel's
  default list

#### Scenario: Unknown policy name rejected
- **WHEN** a policy command names a policy the client does not know
- **THEN** ChanServ replies with an error and the policy list is unchanged

#### Scenario: Action without key refused
- **WHEN** a user issues an owner or admin command without the corresponding
  key configured
- **THEN** ChanServ replies with an error and publishes nothing

### Requirement: Signing keys are node-local secrets

Owner and admin schnorr keypairs SHALL be provisioned in node configuration
(desktop TOML config; app settings store) and MUST NOT be transmitted or
published; only public keys appear in DAG events. Signatures apply to action
and message content only.

#### Scenario: Secret never leaves the node
- **WHEN** any owner or admin action is published
- **THEN** the corresponding event contains only the public key and signature

### Requirement: RLN semantics unchanged

Moderation MUST NOT alter RLN rate-limiting or anonymity: hide actions and
signed messages on RLN-enabled networks are ordinary rotating events subject
to the same proof requirements as chat messages, and no policy data links an
RLN identity to a signing key.

#### Scenario: Hide action is rate-limited
- **WHEN** an admin publishes a hide action on an RLN-enabled network
- **THEN** the event is admitted under the same RLN proof rules as any other
  rotating event
