## Why

The app's entire look — the scifi cyan palette, the king video background,
the scramble splash, overlay fades — is hard-coded inline across ~14 schema
files (233 `set_property_*` styling calls, 125 `text_color` touches) and has
even leaked into the node factories in `node.rs` (`action_fg_color`,
`url_copy_*`, `scramble_color`, menu role colors). The only variation
mechanism is a compile-time `COLOR_SCHEME` switch whose PaperLight arm is
dead code. There is no way to change the look at runtime, no defined
boundary between "style" and "structure", and no reset point themes could
start from. This change makes the UI themable at runtime by layering themes
on the property system's existing defaults/vals fallback, turning the
current look into one selectable theme (`scifi`) over a neutral `minimal`
baseline.

## What Changes

- **Property system (`bin/app/src/prop/`)**: defaults become installable on
  live (already-linked) properties via a post-creation `set_default_*` API
  for every type; defaults may hold SExpr; "effective expr" semantics —
  `is_expr`/`get_expr` fall through `vals` → `defaults`, and `get_value`
  never returns an unresolved SExpr (resolves via cache, else type
  default). Factories opt in `allow_exprs()` for themeable props (it is
  builder-only today, so live properties can never gain expr support).
  `Role` becomes a bitflag set and properties gain
  `PropertyPermission` (read mask, write mask) enforced on writes
  (every mutating API already carries a role) and on attributed reads
  (wrap layer, expr evaluation, RPC boundary), failing with
  `PropertyPermissionDenied` — which turns "themes don't override
  widget-owned properties" from convention into a hard invariant.
- **Expr evaluation beyond rects**: f32-array properties (4-component
  colors, font sizes, spacing) gain per-index expr evaluation against
  dependency globals, re-evaluated in widgets' draw paths alongside rect
  evaluation (today only `PropertyRect` has this); `when_change` handlers
  stay invalidation-only.
- **`/theme` token node**: a scene-root node holding the shared
  themeable vocabulary as token properties whose defaults are the
  `minimal` palette. Schema creation code wires styled properties to
  tokens via helper functions ("classes") that install default-exprs
  plus `add_depend` edges — one wiring, every wired widget tracks the
  token forever, including nodes created at runtime. Themes can also
  mint their own private tokens as tracked child nodes
  (`/theme/<name>`), usable in overrides, gone on unload — themes are
  not boxed in by the shared list.
- **Minimal baseline**: the schema installs Tier-1 defaults (complete,
  neutral, usable) instead of inline styling; factories are scrubbed of
  theme leaks back to neutral Tier-0 type defaults.
- **Theme engine (`bin/app/src/theme/`)**: `Theme` trait + registry;
  `apply`/unload with nodes as the storage unit — theme-created
  properties live on tracked nodes (private tokens under
  `/theme/<name>`), theme tasks are pushed onto those nodes and cancel
  with them; every theme modification is stamped with a new
  `Role::Theme` (attribution on the event stream — `vals` stores no
  authorship); unload cancels theme tasks first, then resets only the
  recorded touched properties (a deep-walk reset is neither possible
  nor safe), restores unbounded-list priors (e.g. `nick_colors`, which
  have no defaults tier), removes theme-added dependency edges, and
  unlinks tracked nodes — all as one atomic property batch. Theme
  behavior (e.g. the netstatus overlay fade) is implemented as property
  watchers, not schema hooks.
- **scifi extracted**: the first real theme — cyan token palette, king
  video background node, fade watchers.
- **`/setting/theme` enum** (`minimal` | `scifi`, default `scifi`),
  persisted through the existing `Setting` pimpl, applied after schema
  load, live-switched on change; the settings screen gains enum
  rendering (enums currently display as "unknown").
- **PaperLight removed**: `ColorScheme` enum, the `COLOR_SCHEME` const, and
  all two-arm style branches (collapsing to the DarkMode values) are
  deleted across 14 files, including the dead window-linked `bg`
  vector-art block.

## Capabilities

### New Capabilities

- `prop-defaults`: post-creation default installation on live properties
  (all types incl. SExpr), effective-expr resolution order for reads, and
  f32-array expr evaluation with dependency-triggered re-evaluation.
- `app-theme`: the runtime theme system — token node and class wiring,
  minimal baseline, theme apply/unload lifecycle (tracked nodes, tasks,
  journal), atomic switching, the `theme` setting, and scifi as first
  theme.

### Modified Capabilities

(none — no existing specs in this repo; the property-system extensions are
new behavior captured in `prop-defaults`.)

## Impact

- `bin/app/src/prop/mod.rs`, `src/prop/wrap.rs` (default APIs, effective
  exprs, f32-multi evaluation) + unit tests in the existing test module.
- `bin/app/src/ui/` — color/font expr re-evaluation call sites in
  widgets' `when_change` update paths.
- `bin/app/src/app/node.rs` (factory default scrub), `src/setting.rs`
  (theme enum), `src/app/schema/settings.rs` (enum rendering).
- `bin/app/src/app/schema/**` (all files: PaperLight removal, minimal
  defaults, token wiring) and `src/app/mod.rs` (apply theme after
  `schema::make`).
- New `bin/app/src/theme/` module.
- No changes outside `bin/app`; no new dependencies. Verified via
  `bin/app` Makefile (`make compile-dev`, `make compile-apk`) and the
  property unit tests.
