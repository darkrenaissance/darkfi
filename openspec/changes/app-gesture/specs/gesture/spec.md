## Purpose

Defines how touch input is recognized into gesture events and delivered to UI
widgets of the app: a single recognition system with unified thresholds, the
widget contract for receiving gestures, and the touch behavior of every
migrated widget.

## ADDED Requirements

### Requirement: Gesture event stream
The system SHALL recognize touch input into a stream of gesture events:
`Down` and `Up` passthrough events delivered immediately at touch start and
end, `Tap`, `LongPress`, and a drag lifecycle of `DragStart`, `DragMove`, and
`DragEnd` carrying the release velocity. All gesture positions SHALL be
delivered in the receiving widget's local coordinate space.

#### Scenario: Immediate feedback at touch start
- **WHEN** a touch begins on a widget that hit-tests at that position
- **THEN** the widget receives a `Down` event without waiting for any gesture
  to resolve

#### Scenario: Drag lifecycle completes exactly once
- **WHEN** a touch moves beyond the drag threshold and is later released
- **THEN** the target widget receives one `DragStart`, zero or more
  `DragMove`, and exactly one `DragEnd` whose velocity reflects the recent
  movement history at release

#### Scenario: Coordinates are local
- **WHEN** a gesture is delivered to a widget nested inside layers
- **THEN** event positions are translated into the widget's own coordinate
  space

### Requirement: Unified recognition thresholds
The system SHALL use one set of recognition constants for all widgets: touch
slop bounding tap travel and long-press stationarity, a tap duration bound,
the system long-press timeout, a drag start threshold equal to touch slop, a
move delivery period of 20ms, and a velocity sample window of 40ms. A `Tap`
SHALL require travel within slop and duration within the bound.

#### Scenario: Tap within slop
- **WHEN** a touch goes down and up within slop travel and within the tap
  duration bound
- **THEN** a `Tap` is delivered

#### Scenario: Movement beyond slop cancels tap
- **WHEN** travel exceeds slop before release
- **THEN** no `Tap` fires and drag recognition proceeds instead

#### Scenario: Move delivery is throttled
- **WHEN** touch moves arrive faster than the move delivery period
- **THEN** `DragMove` is delivered at most once per period while velocity
  sampling still observes every move

### Requirement: Long-press fires during hold
`LongPress` SHALL fire while the finger is still down, once the system
long-press timeout elapses with travel within slop. It SHALL fire at most
once per touch and SHALL be cancelled by travel beyond slop before the
timeout or by touch cancellation.

#### Scenario: Stationary hold fires during contact
- **WHEN** a touch is held past the long-press timeout without exceeding slop
- **THEN** `LongPress` is delivered while the touch is still down

#### Scenario: Movement cancels long-press
- **WHEN** travel exceeds slop before the timeout elapses
- **THEN** no `LongPress` fires for that touch

### Requirement: Touch ownership
The target of a touch SHALL be resolved by hit-testing the widget tree in
priority order at touch start. All gesture events for that touch SHALL be
delivered only to the resolved target chain until the touch ends or is
cancelled. A touch that moves over a different widget mid-gesture SHALL NOT
hand off ownership.

#### Scenario: Ownership is sticky
- **WHEN** a touch starts on widget A and moves over sibling widget B before
  release
- **THEN** only widget A's chain receives events for that touch

#### Scenario: Priority ordering
- **WHEN** two overlapping widgets both hit-test at the touch start position
- **THEN** the higher-priority widget owns the touch

### Requirement: Recognition observes all touches
Gesture recognition SHALL observe every touch event regardless of which
widget handles it, including touches that begin on widgets that previously
claimed events synchronously. Secondary touches (additional simultaneous
touch ids) SHALL NOT alter or cancel recognition of the primary touch.

#### Scenario: Touch on a latency-sensitive widget still recognized
- **WHEN** a touch begins on a widget whose interaction previously suppressed
  event delivery to the recognition system
- **THEN** gesture events for that touch are still recognized and delivered
  to its target chain

#### Scenario: Second finger is inert
- **WHEN** a second touch id appears during an active drag
- **THEN** the active touch's recognition and delivery are unaffected

### Requirement: Gesture arbitration
When multiple widgets in the target chain accept competing gestures, the
first recognizer to resolve SHALL claim the gesture and competing
recognizers SHALL be cancelled. Within slop the descendant's tap wins over an
ancestor's drag; beyond slop the ancestor's drag wins over the descendant's
pending tap.

#### Scenario: Row tap inside a scrollable menu
- **WHEN** a touch on a menu row is released within slop
- **THEN** the row receives the `Tap` and the menu's scroll is not engaged

#### Scenario: Scroll wins on movement
- **WHEN** the same touch instead travels beyond slop
- **THEN** the menu's drag claims the gesture and the row's pending tap is
  cancelled

### Requirement: Widget gesture contract
Widgets SHALL declare the gestures they accept and a hit-test region.
Widgets that declare no gestures SHALL receive only `Down`/`Up` passthrough;
widgets whose hit-test excludes a position SHALL receive no gesture events
for that touch.

#### Scenario: Non-participating widget is inert
- **WHEN** a touch passes over a widget that declares no gestures
- **THEN** that widget receives no recognized gesture events

### Requirement: Touch cancellation
Touch cancellation SHALL tear down all pending recognition state and timers
for that touch, and no further gesture events SHALL be emitted for it after
cancellation.

#### Scenario: Cancelled touch emits nothing further
- **WHEN** the system cancels an active touch mid-gesture
- **THEN** pending long-press timers are invalidated and no `Tap`,
  `LongPress`, or `DragEnd` fires for it

### Requirement: Emulated touch parity
Mouse-emulated touches on desktop SHALL produce the same gesture recognition
and delivery as real touches on Android.

#### Scenario: Desktop emulated tap
- **WHEN** a click is performed through mouse emulation of touch
- **THEN** the same `Tap` is recognized and delivered as on device

### Requirement: Button and token table activation
The button SHALL emit its click signal on `Tap` within its hit region, and
the token table SHALL emit its row click signal on `Tap` within a row.
Activation by mouse remains unchanged.

#### Scenario: Button tap activates
- **WHEN** a `Tap` lands inside the button's hit region
- **THEN** the button's click signal fires

### Requirement: Emoji picker scroll and selection
The emoji picker SHALL scroll one-to-one with vertical drag after slop,
clamped to its scroll bounds, and SHALL activate the emoji under a `Tap`.

#### Scenario: Drag scrolls, tap selects
- **WHEN** a vertical drag moves within the picker
- **THEN** scroll follows the finger clamped to bounds and no emoji is
  activated; on `Tap` within slop the emoji under the position is activated

### Requirement: Menu edit mode, selection, and reorder
The menu SHALL enter edit mode on `LongPress`, SHALL select or delete items
on `Tap`, and SHALL reorder items via a drag armed by touching the reorder
handle at touch start.

#### Scenario: Long-press enters edit mode
- **WHEN** a touch is held on the menu past the long-press timeout within
  slop
- **THEN** edit mode activates while the finger is still down

#### Scenario: Reorder drag
- **WHEN** a touch starts on an item's reorder handle in edit mode and moves
- **THEN** the item's insertion index follows the drag and the reorder
  commits at touch end

### Requirement: Chat view scroll, selection, and taps
The chat view SHALL scroll one-to-one with vertical drag, SHALL drive its
scroll inertia from the `DragEnd` velocity, SHALL stop inertia when a new
drag starts, SHALL start line selection or show a URL toast on `LongPress`,
and on `Tap` SHALL forward to the message under the touch (opening URLs,
downloading files) or toggle line selection when selection is active.

#### Scenario: Flick continues after release
- **WHEN** a fast vertical drag is released
- **THEN** the view keeps scrolling with inertia derived from the release
  velocity and decays to a stop

#### Scenario: Grab stops inertia
- **WHEN** a new touch begins during inertial scrolling
- **THEN** inertia stops and the new drag owns the scroll

#### Scenario: Tap forwards to message content
- **WHEN** a `Tap` lands on a message URL
- **THEN** the URL opens and the view does not treat it as a scroll

### Requirement: Text edit hybrid gestures
The text edit SHALL arm selection-handle dragging at `Down` by grabbing a
handle within its radius, SHALL select the word under the touch and show the
action menu on `LongPress`, SHALL set the cursor and request focus on `Tap`,
and SHALL scroll vertically on vertical drag while fingers move text-selection
handles.

#### Scenario: Long-press selects word
- **WHEN** a touch is held on the edit past the long-press timeout within
  slop
- **THEN** the word under the touch is selected and the copy/paste menu is
  shown while the finger is still down

#### Scenario: Handle drag adjusts selection
- **WHEN** a touch starts within a selection handle's grab radius and moves
- **THEN** the selection endpoint follows the finger from the first movement

### Requirement: Touch interaction expressed only through gestures
After migration, widget touch interaction SHALL be expressed exclusively
through the gesture stream. The per-widget raw phase handlers and the
separate synchronous touch dispatch path SHALL NOT exist.

#### Scenario: Single dispatch path
- **WHEN** any touch event is processed
- **THEN** it feeds one recognition system and gestures are delivered through
  one path in one ordering
