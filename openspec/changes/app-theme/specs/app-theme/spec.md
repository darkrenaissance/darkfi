## Purpose

Runtime theme system for the app UI: a neutral minimal baseline installed
as property defaults, a token vocabulary wired into styled properties, and
a switchable theme lifecycle (apply, unload, live switch) layered on top —
so the entire visual style, including theme-owned decorations and
animations, can change at runtime without restart.

## ADDED Requirements

### Requirement: Theme selection setting

The app SHALL expose a `theme` setting under `/setting` as an enum of the
available theme names, persisted across restarts. The default SHALL be
`scifi`. A persisted value that is not among the available themes SHALL
be treated as the default.

#### Scenario: First run defaults to scifi

- **WHEN** the app starts with no persisted theme setting
- **THEN** the active theme is scifi and the UI matches the current shipped look

#### Scenario: Selection persists across restart

- **WHEN** the user switches the theme and later restarts the app
- **THEN** the persisted theme is active on startup

#### Scenario: Unknown persisted value falls back

- **WHEN** the persisted theme is not an available theme (e.g. removed in a
  later version)
- **THEN** the app starts with the default theme instead of failing

### Requirement: Theme application at startup

After the schema is constructed, the persisted theme SHALL be applied
before the first drawn frame. With `minimal` selected, the UI SHALL render
the minimal baseline with no theme decorations.

#### Scenario: App starts themed

- **WHEN** the app starts with any valid persisted theme
- **THEN** the first drawn frame already reflects that theme

#### Scenario: Minimal at startup

- **WHEN** the app starts with `minimal` persisted
- **THEN** the UI renders the baseline with no theme-owned nodes (no video
  background, no splash, no themed fades)

### Requirement: Layered theming over a minimal baseline

The minimal baseline SHALL be installed as property defaults during schema
construction. A theme SHALL override properties by setting values (plain
or expression) in the value slot, and MAY override any property type,
including rect geometry. Unloading a theme SHALL restore the baseline for
every property it touched, without recreating nodes or restarting.

#### Scenario: Rect override restores on switch

- **WHEN** a theme overrides a widget's rect and the theme is later unloaded
- **THEN** the widget's rect returns to the baseline geometry

#### Scenario: No styling residue after unload

- **WHEN** a theme that set colors, fonts, and spacing is unloaded
- **THEN** every property it touched reads its baseline value

### Requirement: Atomic theme switching

Switching themes SHALL unload the current theme and apply the new one as a
single atomic property modification batch, followed by a redraw. The UI
SHALL reflect only the new theme afterward, with no restart and no
permanently mixed state.

#### Scenario: Live switch end to end

- **WHEN** the user switches from scifi to minimal while a screen is open
- **THEN** in the next redraw the whole UI shows the minimal look (accents,
  background, and decorations all changed together)

### Requirement: Theme-created node and task teardown

Nodes created by a theme SHALL be tracked, and on unload SHALL be removed
from the scene tree with their tasks cancelled. Tasks spawned by a theme
(including animations reacting to property changes) SHALL stop when the
theme unloads.

#### Scenario: Theme background node disappears

- **WHEN** switching away from a theme that inserted a background node
- **THEN** the node is gone from the scene tree and nothing it drew remains

#### Scenario: In-flight animation stops on switch

- **WHEN** a theme-driven fade animation is mid-flight and the theme is
  switched away
- **THEN** the animation stops making further changes

### Requirement: Token propagation to late-created nodes

Changing a theme token SHALL update every property wired to that token.
Nodes created after a theme is applied (e.g. a chat screen for a channel
joined later) SHALL render with the active theme's styling without any
re-theming pass.

#### Scenario: Token change updates all wired properties

- **WHEN** a token value changes while the app is running
- **THEN** all properties wired to that token reflect the new value after the
  next evaluation

#### Scenario: Late-created node is themed

- **WHEN** a channel is joined and its chat screen constructed while a
  non-minimal theme is active
- **THEN** the new screen renders with the active theme's styling

### Requirement: Minimal baseline completeness

The minimal baseline SHALL be a complete, usable look on its own: all
user-visible surfaces (chat, channel/contact menus, wallet flows, send
and receive screens, settings, netstatus overlay) SHALL render with
readable text and visible controls when only the baseline is active.

#### Scenario: Full walkthrough under minimal

- **WHEN** the app runs with the minimal theme and the user visits each
  surface
- **THEN** text is legible against its background and interactive controls
  are visible and usable on every surface

### Requirement: Theme-defined tokens

A theme SHALL be able to define additional token properties beyond the
shared vocabulary, existing only while that theme is applied. Theme
overrides and expressions SHALL be able to reference these tokens. On
unload, theme-defined tokens SHALL cease to exist along with the
dependencies referencing them, leaving no residue.

#### Scenario: Private token drives an override

- **WHEN** a theme defines a private token, wires a widget property to
  it, and the token's value is set
- **THEN** the widget renders per the private token's value while the
  theme is active

#### Scenario: Private tokens vanish on switch

- **WHEN** the theme that defined private tokens is unloaded
- **THEN** those tokens and any dependency edges referencing them are
  gone, and properties that referenced them read their baseline values

### Requirement: Restoration of non-expression themeable properties

For themeable properties that cannot hold expressions (e.g. unbounded
value lists such as chat nick colors), a theme SHALL record the prior
value before setting it and restore that value on unload.

#### Scenario: Nick colors restore after switching away

- **WHEN** a theme sets a nick color list and is then unloaded
- **THEN** the previous nick color list is restored
