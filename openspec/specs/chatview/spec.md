## Purpose

The chatview2 widget renders a channel's message history as a virtualized,
scrollable chat log with typed interactive messages, replacing the current
`bin/app` chatview. This spec defines its observable behavior: scene API,
buffer semantics, scrolling, storage, loading, filtering, resource
management, and feature parity with the current implementation.

## Requirements

### Requirement: Single chat screen with channel retargeting

The chatview SHALL support rebinding to a different channel's message
store at runtime via a `set_channel` method. On exit from a channel it
SHALL release that channel's in-memory buffer; on entry it SHALL reload
the target channel's messages through the background loading pipeline.
Message-type sub-nodes and their signal wirings SHALL remain attached and
functional across channel switches.

#### Scenario: Switching channels clears and reloads

- **WHEN** `set_channel` is called with a channel that has stored history
- **THEN** the previously displayed messages are no longer rendered and
  the target channel's newest messages load in the background

#### Scenario: Signal wirings survive channel switches

- **WHEN** the UI has subscribed to a message-type sub-node signal and the
  channel is switched
- **THEN** the subscription remains active and receives signals from
  messages of the newly bound channel

#### Scenario: Entering a channel with no history

- **WHEN** `set_channel` targets a channel with an empty store
- **THEN** the view renders empty and remains interactive

### Requirement: Scroll position restore on re-entry

The chatview SHALL remember, per channel, where the user left off and
restore that position on re-entry. The remembered state SHALL identify the
message being viewed (anchor id plus pixel offset) so restoration is stable
when messages arrive while the channel is not open. A user who left the
channel at the bottom SHALL return to the bottom.

#### Scenario: Re-entry restores the same content

- **WHEN** the user exits a channel while scrolled into history and new
  messages arrive before re-entry
- **THEN** the restored view shows the same anchor message at the same
  offset within the viewport

#### Scenario: Re-entry at the bottom

- **WHEN** the user exits a channel while at the live bottom
- **THEN** re-entry restores the bottom position and newly arrived
  messages are visible

#### Scenario: Anchor no longer available

- **WHEN** the anchored message cannot be found on re-entry
- **THEN** the scroll position is clamped to a valid position without
  crashing or blocking

### Requirement: Render resources released when out of view

The chatview SHALL release render resources (meshes, glyphs, textures,
layouts) for messages that leave the visible region, using a soft window
plus LRU budget rather than strict window-bound eviction. Scrolling
through a large history SHALL NOT cause unbounded growth of memory or GPU
resource usage. Render-scoped async tasks SHALL be cancelled when their
message is released.

#### Scenario: Long scroll does not accumulate resources

- **WHEN** the user scrolls through many screens of history
- **THEN** resources held for messages far outside the viewport are
  released, and total resource usage stays bounded

### Requirement: Buffer-size independent interaction

Geometry queries (total content height, the set of messages visible at a
scroll position, the position of a given message) and per-frame scrolling
work SHALL NOT scale with the number of buffered messages. UI interaction
responsiveness SHALL NOT degrade as the buffer grows.

#### Scenario: Scrolling a large buffer costs like a small one

- **WHEN** the same viewport is scrolled by the same delta with a small
  and then a very large loaded buffer
- **THEN** per-frame cost and responsiveness are comparable

### Requirement: Scroll semantics in pixels from the bottom

The scroll position SHALL be measured in pixels from the live bottom of
the content (`0` = bottom, increasing = further up in history) and
SHALL be clamped to the valid range. The position SHALL be internal
view state, not a settable scene property: externally, the view SHALL
expose a `scroll_to_bottom` method and an at-bottom indication that
distinguishes "at the live bottom" from "scrolled into history".

#### Scenario: Scroll zero pins to live bottom

- **WHEN** the view is at the bottom and a new message arrives
- **THEN** the view stays at the bottom and the new message is visible

#### Scenario: Clamping

- **WHEN** a gesture or animation requests a scroll position beyond the
  valid range
- **THEN** the resulting position is clamped and no error occurs

#### Scenario: Scroll to bottom

- **WHEN** `scroll_to_bottom` is invoked (e.g. the down-arrow button)
- **THEN** any in-flight motion stops and the view returns to the live
  bottom; the at-bottom indication reflects the position

### Requirement: Direct-drag scrolling

Touch-drag scrolling, delivered by the app gesture subsystem as a drag
lifecycle (`DragStart`/`DragMove`/`DragEnd`), SHALL move content 1:1
with the pointer in pixels, without animation or smoothing on top. The
slop dead-zone before the drag starts and the move delivery cadence
SHALL come from the gesture session, not the view. Starting a drag —
or touching down over the view — SHALL cancel any in-flight glide or
scroll animation.

#### Scenario: Finger tracking is pixel-exact

- **WHEN** the finger moves up by N pixels during a drag
- **THEN** the content scrolls exactly N pixels, delivered at the
  gesture session's move cadence, with no acceleration or smoothing
  added by the view

#### Scenario: Slop dead-zone precedes scroll

- **WHEN** a touch travels less than the session's touch slop
- **THEN** no scroll occurs and the touch remains eligible for
  tap/long-press recognition

#### Scenario: Grabbing stops motion

- **WHEN** a touch begins while an animated scroll or glide is in progress
- **THEN** the animation/glide stops immediately and the drag takes over

### Requirement: Animated page scrolling for wheel and keys

Mouse wheel ticks and PageUp/PageDown SHALL scroll half a page, animated
with easing. Repeated ticks while an animation is in flight SHALL
retarget/coalesce the animation rather than accumulate velocity.

#### Scenario: Single wheel tick

- **WHEN** the mouse wheel is scrolled one tick
- **THEN** the view animates half a page in the wheel direction

#### Scenario: Repeated ticks coalesce

- **WHEN** several wheel ticks occur in quick succession
- **THEN** the animation target extends by half a page per tick and the
  motion remains smooth, without a velocity runaway

### Requirement: Flick inertia

Releasing a drag with sufficient velocity SHALL produce an inertial glide
that decays over time and stops within the clamped range. Release
velocity SHALL be taken from the gesture session's `DragEnd` velocity
(the view does not sample its own). Touching down over the view during a
glide SHALL stop it, including before the touch travels past the touch
slop.

#### Scenario: Flick decays and stops

- **WHEN** the finger is released with upward velocity
- **THEN** the content glides in the same direction, decaying, and comes
  to rest at or within the valid scroll range

#### Scenario: Touchdown stops a glide

- **WHEN** the user touches the view while an inertial glide is in motion
- **THEN** the glide stops immediately, even if the touch never travels
  past the touch slop

### Requirement: Scroll compensation for height changes

When a message below the viewport bottom changes height, the scroll
position SHALL be adjusted by the height delta so the viewed content stays
stable, unless scroll is `0` (bottom pinned). Height changes inside the
viewport SHALL grow or shrink the content around the current view without
jumping. Total height and the maximum scroll SHALL reflect height changes
immediately.

#### Scenario: Image loads below the reading position

- **WHEN** the user is scrolled into history and a message below the
  viewport grows as its image loads
- **THEN** the viewed content does not move

#### Scenario: Expansion inside the viewport

- **WHEN** the user expands a collapsed message that is on screen
- **THEN** the message expands in place without the surrounding content
  jumping out of view

### Requirement: Message type sub-nodes

Each message type SHALL be represented by exactly one sub-node of the
chatview, exposing type-specific styling properties, signals whose
payloads identify the message (msg id) plus type-specific data, and
methods. Message lifecycle operations defined by each type's own
semantics (e.g. inserting messages of that type, file status updates)
SHALL be methods and signals of the type's sub-node; the chatview node
SHALL expose only view-wide methods and signals (channel switching,
filtering, selection). Sub-nodes SHALL NOT be created or destroyed when
the buffer changes (channel switch, load, eviction). Registering a new
message type SHALL NOT require modifying existing types.

#### Scenario: Nick click emits an identified signal

- **WHEN** the user clicks a nick inside a privmsg
- **THEN** the privmsg type sub-node emits a signal carrying the msg id
  and nick, which the UI can use (e.g. inserting the nick into the chat
  editor)

#### Scenario: Lifecycle operations go through type nodes

- **WHEN** a privmsg is inserted, confirmed, or a file status changes
- **THEN** the operation is invoked as a method on the corresponding
  type sub-node (privmsg insert/confirm, filemsg status), not on the
  chatview node

#### Scenario: New message type is additive

- **WHEN** a new message type is registered with its sub-node and payload
  decoder
- **THEN** existing types and stored messages continue to work unchanged

### Requirement: Debug introspection and deletion

The chatview SHALL provide methods to support live testing:
enumerating the ids (with timestamps) of currently loaded messages in
display order, and deleting a loaded message by id. Deletion SHALL
remove the message from the buffer (updating ordering, geometry, and
rendered state correctly) and from the channel's storage. These methods
are testing affordances, not user-facing features.

#### Scenario: Enumerate loaded messages

- **WHEN** the id-enumeration method is called
- **THEN** it returns the ids and timestamps of all currently loaded
  messages in display order

#### Scenario: Delete by id updates everything

- **WHEN** a loaded message is deleted by id via the deletion method
- **THEN** it disappears from the view, geometry (total height, scroll
  range) updates correctly, and it is absent after the channel is
  re-entered

### Requirement: Styling inheritance and regen

Styling properties shared across message types (e.g. font size, line
height, timestamp styling, selection color) SHALL be defined once on the
chatview node. A type sub-node SHALL only define properties specific to
it and MAY override an inherited property by defining its own. Message
types SHALL receive live property handles when created; changing a styling
property SHALL cause affected messages' rendered state to be rebuilt
(regen) with re-measured heights.

#### Scenario: Font size change re-renders everything

- **WHEN** the chatview font size property changes
- **THEN** all rendered messages are re-laid-out at the new size, heights
  are re-measured, and the scroll position remains valid (compensated)

#### Scenario: Type-specific override

- **WHEN** a type sub-node defines its own value for an otherwise
  inherited property
- **THEN** messages of that type render using the override while other
  types use the inherited value

### Requirement: Async message content updates

Message types MAY run async tasks that update their own persistent data
(e.g. download progress, decoded image buffers) and then rebuild their
rendered state, including height. Tasks tied to rendering SHALL be
cancelled on release. Tasks that must outlive eviction (e.g. background
downloads) SHALL be hosted on the type sub-node, keyed by msg id or
content address so duplicates are not spawned, and surviving tasks SHALL
keep updating state; re-materializing a message SHALL attach to running
tasks or current state rather than restart from scratch.

#### Scenario: Download continues while evicted

- **WHEN** a file message's download task is running and the message
  scrolls out of view
- **THEN** the download continues, and scrolling back shows current
  progress without a duplicate task

#### Scenario: Content update changes height

- **WHEN** an image finishes loading and replaces a progress placeholder
- **THEN** the message re-renders at its new height and scroll
  compensation keeps the view stable per the height-change rules

### Requirement: Random insertion at any timestamp

Messages SHALL be insertable at any timestamp position (including
backfill of older messages during sync) with the view updating correctly.
Buffer ordering SHALL use the (timestamp, msg_id) composite key.
Inserting a message whose (timestamp, msg_id) already exists SHALL be
ignored (deduplication).

#### Scenario: Backfilled message appears in order

- **WHEN** an older message arrives while the user views history that
  includes its timestamp position
- **THEN** it appears at the correct chronological position without
  duplicating or displacing other messages

#### Scenario: Same-millisecond messages coexist

- **WHEN** two messages share a timestamp but have different msg ids
- **THEN** both are stored, ordered, and rendered

#### Scenario: Duplicate insert is ignored

- **WHEN** the same (timestamp, msg_id) is inserted twice
- **THEN** the second insert has no visible effect

### Requirement: Persisted message storage format

Messages SHALL be persisted in the per-channel kvdb tree with key
`(timestamp big-endian, msg_id)` and value `[type_id][type-owned
bytes]`; the type decides how to interpret its bytes. Unconfirmed
messages SHALL be persisted with a confirmed flag (type-owned payload
state), and their later confirmation SHALL update the existing entry in
place rather than create a duplicate. The format is a clean break from
the previous chatview: values it cannot decode are corrupt data and
SHALL fail explicitly (panic) rather than be skipped or misread.

#### Scenario: Unconfirmed then confirmed

- **WHEN** a message is sent unconfirmed and later confirmed via the
  privmsg type node's confirm method
- **THEN** exactly one stored entry exists, rendered with confirmed
  styling, and it survives app restart

#### Scenario: Corrupt entry fails explicitly

- **WHEN** a stored value carries an unknown type id or undecodable
  payload
- **THEN** loading fails loudly with an explicit error identifying the
  entry, never a silent skip

### Requirement: Background loading pipeline

All message loading — channel entry, live receive, scrolling toward
history, and filter changes — SHALL happen in an async background pipeline
that never blocks UI interaction. The loader SHALL maintain coverage of
the visible region plus a preload margin, waking on demand.

#### Scenario: Entering a large channel is non-blocking

- **WHEN** `set_channel` targets a channel with a large history
- **THEN** the UI is immediately interactive while messages stream in

### Requirement: Runtime message filter

A filter callback SHALL be settable and replaceable at runtime. The filter
SHALL decide which stored messages enter the buffer during loading;
filtered-out messages SHALL still be persisted. Changing the filter SHALL
rebuild the visible set through the background pipeline.

#### Scenario: Filter narrows the view

- **WHEN** a filter that excludes some messages is set and the buffer
  reloads
- **THEN** excluded messages are not rendered but remain in storage

#### Scenario: Filter replaced at runtime

- **WHEN** the filter callback is swapped
- **THEN** the view rebuilds in the background using the new filter

### Requirement: Privmsg rendering parity

Privmsgs SHALL render with the current chatview's feature set: nick
coloring (stable per nick), CTCP ACTION rendering, NOTICE styling with
reduced font size, unconfirmed gray styling, timestamps, URL detection
with colored/backgrounded spans, URL click/tap opening, and URL
right-click or long-press copying with the "copied link" toast overlay.

#### Scenario: URL interaction parity

- **WHEN** a message containing a URL is clicked, or long-pressed on
  touch
- **THEN** the URL opens (click) or is copied with the toast overlay
  (long-press/right-click), matching current behavior

### Requirement: Selection across message types

Any displayed message, regardless of type, SHALL be selectable (mouse
click toggle, `Tap` toggle, drag sweep, long-press entering selection
mode with subsequent drag extending the selection, tap toggling in
selection mode). `copy_select`
SHALL copy the selected messages' text in display order joined by
newlines, where each selected message contributes copy text defined by
its type; a type MAY contribute nothing. `unselect` SHALL clear all
selection. `select_changed` SHALL fire on transitions between having
and not having any selection.

#### Scenario: Mixed-type selection copies per-type text

- **WHEN** a privmsg, a file message, and a date separator are all
  selected and `copy_select` is invoked
- **THEN** the clipboard contains each message's type-defined copy
  text (privmsg its rendered line, file message its file URL, date
  separator its date label) joined by newlines in display order, and
  selection is cleared

#### Scenario: Every type toggles

- **WHEN** a date separator line is clicked
- **THEN** it becomes selected and shows the selection highlight,
  unlike the current chatview where separators are unselectable

#### Scenario: Long-press selects and drag extends

- **WHEN** a long-press lands on a message line and the finger then
  drags without lifting
- **THEN** the pressed line becomes selected, selection mode is entered,
  and the drag extends the selection instead of scrolling

#### Scenario: Selection transitions signal

- **WHEN** the first line becomes selected, or the last selection is
  cleared
- **THEN** `select_changed` fires with true and false respectively

### Requirement: Expandable message height

Long messages SHALL be capped to a default height with an affordance to
expand to full height; expansion changes the message height and follows
the height-change scroll rules. In v1 the privmsg body is plain text with
nicks and URLs; the payload format and APIs SHALL accommodate a richer
span/block body (quotes, styling, code, math) added later without
breaking stored messages or the buffer/scroll machinery.

#### Scenario: Long message collapses and expands

- **WHEN** a message taller than the cap is rendered and the user toggles
  expansion
- **THEN** the message renders capped, then expands in place following
  the height-change rules, and collapsing restores the capped height

### Requirement: File message parity

fud file messages SHALL be derived from privmsg text containing fud URLs,
render the file status lifecycle (initializing, idle, downloading with
progress, downloaded, error), request downloads on click/tap via a signal,
update rendering when status changes, and display downloaded images
scaled to fit width and height bounds.

#### Scenario: File download lifecycle

- **WHEN** a file message is tapped while idle and the download progresses
- **THEN** a download request signal fires, status renders with progress,
  and on completion the image (or completed state) is displayed

### Requirement: Reflow on viewport changes

When the viewport width or window scale changes, the chatview SHALL
re-wrap affected messages, re-measure heights, and keep the user's
reading position stable: the anchored content remains in view after
reflow, and a bottom-pinned view stays bottom-pinned. Height-only
viewport changes (e.g. the input editor growing) SHALL NOT trigger
re-wrapping. Reflow SHALL complete without losing messages.

#### Scenario: Width change keeps reading position

- **WHEN** the window is resized while the user is reading history
- **THEN** messages re-wrap at the new width and the message under the
  viewport anchor remains in view at the same offset

#### Scenario: Bottom stays bottom across reflow

- **WHEN** a reflow occurs while the view is pinned at the live bottom
- **THEN** the view remains pinned at the bottom

#### Scenario: Height-only change does not rewrap

- **WHEN** the chat editor grows and only the viewport height changes
- **THEN** no re-wrapping occurs and the scroll position remains valid

### Requirement: Date separators and derived messages

Date separator messages SHALL be derived from the stored messages' dates,
never persisted, and inserted into the display order at day boundaries.

#### Scenario: Day boundary renders a separator

- **WHEN** consecutive messages span midnight
- **THEN** a date separator renders between them showing the new date

### Requirement: i18n support

Message types SHALL accept the i18n translation fish and use it for
translatable user-facing strings (e.g. file status labels).

#### Scenario: File status string is translated

- **WHEN** the active language differs and a file message shows "tap to
  download"
- **THEN** the string is rendered translated

### Requirement: Keyboard scrolling

PageUp/PageDown (and wheel equivalents) SHALL scroll via the animated page
scrolling behavior, and keyboard input SHALL be consumable so it does not
leak to other UI elements while interacting with the chatview.

#### Scenario: PageUp animates half a page

- **WHEN** PageUp is pressed
- **THEN** the view animates half a page up and the key event is consumed

### Requirement: Gesture subsystem integration

The chatview SHALL receive touch input exclusively through the app
gesture subsystem: it SHALL declare its accepted gestures via
`gesture_set` (tap, long-press, vertical drag), pass an exact
`gesture_hit_test` matching its rect, and consume the `GestureAction`
stream via `handle_gesture`. It SHALL NOT implement its own recognition
thresholds, long-press timers, or velocity sampling; touch slop, tap
bounds, long-press firing, move throttling, and release velocity SHALL
come from the session. Interactive layers floating over the chatview
(e.g. the scroll-to-bottom arrow) SHALL carry a higher node priority
than the chatview so the session's priority-ordered target resolution
delivers their gestures to them.

#### Scenario: Overlaying arrow receives its taps

- **WHEN** the scroll-to-bottom arrow is visible over the chatview and
  tapped
- **THEN** the arrow receives the tap (triggering scroll-to-bottom) and
  the chatview does not treat it as selection or content activation

#### Scenario: No local recognition

- **WHEN** a touch stream is delivered to the chatview
- **THEN** slop gating, tap/long-press discrimination, long-press
  timing, and release-velocity sampling are those of the gesture
  session; the chatview applies only scroll physics and content
  dispatch
