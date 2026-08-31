## 1. Remove PaperLight (dead code)

- [ ] 1.1 Delete `ColorScheme`, the `COLOR_SCHEME` const, and the dead
  window-linked `bg` vector-art block in `src/app/schema/mod.rs`; collapse
  the `if COLOR_SCHEME == ...` background branch to the unconditional
  video path. Verify `make compile-dev` in `bin/app`.
- [ ] 1.2 Collapse every `match COLOR_SCHEME` / `if COLOR_SCHEME ==`
  two-arm branch in `chat.rs`, `menu/{mod,channel,contact}.rs`,
  `settings.rs`, and `wallet/*.rs` to the DarkMode values inlined, and
  drop the now-unused `ColorScheme`/`COLOR_SCHEME` imports (14 files
  total). Verify `make compile-dev` and that the app renders as before.

## 2. Property default APIs and permissions

- [ ] 2.1 Add post-creation default installation on `PropertyPtr` in
  `src/prop/mod.rs` (`set_default_*` for bool, u32, f32, str, enum, node
  id, shape, null) mutating `defaults[i]` with no modify event, enforcing
  the same length/type checks as the builder variants. Add unit tests:
  install-on-live-node changes effective read; explicit value wins;
  unset falls back to installed default.
- [ ] 2.2 Add expression defaults: builder `set_defaults_expr` plus
  post-creation `set_default_expr`, and make `is_expr`/`get_expr` resolve
  the effective expression source (`vals` expression, else `defaults`
  expression). Update `get_value` so it never returns an unresolved
  expression (resolution order per the `prop-defaults` spec; unevaluated
  expression reads yield the type default). Unit tests: override+unset
  returns to default expression; value-slot expression wins over default
  expression; concrete-type read before first evaluation succeeds.
  Verify `cargo test prop::` in `bin/app` (crate `darkfi-app`) and
  `make compile-dev`.
- [ ] 2.3 Make `Role` a hand-rolled u8 bitflag set (User, App, Internal,
  Ignored, Theme — no new dependency) and add `PropertyPermission`
  {read, write} taken by `Property::new` (transitional default = all
  roles, so all current call sites keep behavior) plus `can_read`/
  `can_write` and `Error::PropertyPermissionDenied` (design D13).
  Enforce write masks centrally in every mutating API (role is already
  a parameter) before any mutation or journaling; enforce read masks in
  the wrap layer (`wrap()` validates upfront, `get()` checks), in
  `eval_f32_multi` dependency reads, and at the `net.rs` RPC boundary;
  exempt `set_default_*` and `set_cache_*`. Unit tests: denied write
  leaves value unchanged; denied wrapped read errors; theme role denied
  on a mask without Theme. Verify `cargo test prop::` and `make
  compile-dev`.

## 3. f32-array expression evaluation beyond rect

- [ ] 3.1 Generalize the `PropertyRect::eval_with` pattern in
  `src/prop/wrap.rs` into a shared bounded f32-array evaluator and expose
  `PropertyColor::eval` (globals-only, no extras) plus a single-f32
  variant for `font_size`/spacing. Unit tests: per-index exprs recompute
  from dependency globals; plain indices untouched; evaluated results
  land in the cache and are returned by `get_f32` (note: cache writes
  bypass `set_f32` range validation by design). Verify `cargo test
  prop::`.
- [ ] 3.2 Add draw-path re-evaluation call sites in color/font-consuming
  widgets (`Text`, `Edit`, `ChatView`, `Menu`, `TextScramble`): at the
  top of `draw()`, alongside the existing rect evaluation, re-evaluate
  expr-bound styled props (`PropertyColor::eval`, f32 eval) so every
  pass computes from current dependency values; keep `when_change`
  handlers invalidation-only (clear draw cache + trigger), with cache
  writes as `Role::Internal` so `when_change_external` echo-skips them
  (same pattern as rect evals today). Verify with a scratch wiring
  (expr-bound `text_color` reacting to a test property) under `make
  compile-dev`.

## 4. `/theme` token node and class-wiring helpers

- [ ] 4.1 Create the `/theme` node (linked at scene root beside
  `/setting`) with an initial token vocabulary: `text_color`,
  `text_dim_color`, `accent_color`, `bg_color`, `edit.*` colors,
  `menu.*` colors, `chatview.*` colors, font/spacing f32s — defaults
  form the neutral minimal palette; write mask = Theme only (D13).
  Node is created in the app setup path before `schema::make`. Verify
  node exists at `/theme` with defaults via a startup log or debug
  lookup; `make compile-dev`.
- [ ] 4.2 Add wiring helpers (`wire_color`, `wire_f32`) that install
  default-exprs + `add_depend` edges from a node property onto token
  components in one call, replacing 4×`set_f32` styling blocks. All
  wiring happens at creation (design D5); theme-time rewiring goes
  through `ctx.depend_journaled`. Verify a helper-wired `text_color`
  tracks a token change at runtime under `make compile-dev`.

## 5. Minimal baseline: schema split and factory scrub

- [ ] 5.1 Scrub theme leaks in `src/app/node.rs` factories back to
  neutral Tier-0 defaults: `action_fg_color`/`action_bg_color`,
  `url_copy_*` colors, `scramble_color`, menu `role1/role2` colors,
  `tokentable` colors; enable `allow_exprs()` on every themeable prop
  (colors, `font_size`, `padding`, `lineheight`, spacing) — it is
  builder-only, so without this the wiring helpers' `set_default_expr`
  calls fail with `PropertySExprNotAllowed`; and assign real
  `PropertyPermission` masks per the D13 table (themeable style =
  write App|Theme; widget-computed/runtime = write Internal without
  Theme, enforcing D4; behavior toggles = write App; content/data =
  write App) — replacing the transitional allow-all default. Verify
  `make compile-dev` and note any visual deltas are restored later via
  tokens/scifi.
- [ ] 5.2 Convert `src/app/schema/mod.rs` (root, netstatus icons and
  overlay) to minimal defaults + token wiring: inline styling blocks
  become `set_default_*` or helper wirings; geometry exprs move to
  default-exprs; the first-run splash block is removed from the schema
  (re-implemented by scifi in 7.1 per design D12). Verify both a
  token-flip recolors the overlay and that with untouched tokens the UI
  looks minimal-but-correct; `make compile-dev`.
- [ ] 5.3 Convert `menu/` (mod, channel, contact, edit helpers) to
  minimal defaults + tokens. Verify menu, channel/contact lists, and
  edit mode render correctly under the baseline; `make compile-dev`.
- [ ] 5.4 Convert `chat.rs` per-channel screens (chatview styling, edit
  composer, selection overlay) to minimal defaults + tokens, including
  journaled handling for `nick_colors` (unbounded list, no exprs).
  Verify a freshly joined channel's screen renders themed-if-tokens-set;
  `make compile-dev`.
- [ ] 5.5 Convert `wallet/` screens (main, send steps 1-4, receive,
  tx_status, data, util helpers) and `settings.rs` to minimal defaults +
  tokens. Verify full wallet flow walkthrough under baseline; `make
  compile-dev`.

## 6. Theme engine

- [ ] 6.1 Create `src/theme/` with the `Theme` trait (`name()`, async
  `apply(ctx)`), compile-time registry (`minimal`, `scifi`), and
  `ThemeCtx` using nodes-as-storage (design D8): tracked node roots
  (incl. theme-defined token children under `/theme/<name>`, built
  pre-Arc with builder `add_property`), theme tasks pushed onto
  theme-owned nodes (`push_task` → cancelled by `clear_tasks()` on
  unload), and a journal of `Touched` (prop, i — unset on unload),
  `List` (unbounded-list priors, e.g. nick_colors — unbounded props
  have no defaults tier), and `Depend` (theme-added edges — removed on
  unload). All ctx setters stamp `Role::Theme` on the modify events
  (attribution lives on the event stream since `vals` has none); verify
  `when_change_impl`'s equality filters pass `Theme` through like `App`
  (only Internal/Ignored are skipped) and audit other Role consumers
  (`net.rs`, `setting.rs` produce roles; any external Role mapping
  needs a `Theme` arm); optional dev-mode leak audit warns on
  `Role::Theme` events for (prop, i) pairs not in the touched-set. The
  touched-set is required for correctness (`vals` has no authorship; a
  deep-walk reset would destroy runtime state) — the ctx is the only
  sanctioned path for themes to modify properties they don't own (and
  D13 masks deny off-path writes outright). Unload order:
  `clear_tasks()` on tracked nodes FIRST (cancel actors — an in-flight
  watcher writing after its unset would leave stale vals) → unset
  Touched → restore Lists → remove Depend edges → unset shared tokens →
  `unlink()` per tracked node. Verify a synthetic theme that touches
  one token, defines one private token wired to a widget, overrides a
  bounded prop and an unbounded list, inserts one node, and pushes one
  watcher task unloads cleanly (second apply leaves no residue, no
  dangling edges, watcher cancelled) via a temporary schema-test hook
  or unit test; `make compile-dev`.
- [ ] 6.2 Implement the atomic switch flow per design D9: single
  `redraw.make_guard` batch (unload + apply), explicit `trigger()` after
  the drop for structural-only changes (unlinks notify nothing); wire it
  to the `/setting/theme` property — `apply_startup()` in `App::setup`
  after `schema::make`, plus an engine-owned watcher task (not
  theme-tracked) for live switches. Verify startup applies the persisted
  theme before the first frame (pubsub queues buffer events until widget
  tasks start; the first pass evaluates draw-side) and a live setting
  change switches atomically; `make compile-dev`.

## 7. scifi theme

- [ ] 7.1 Extract the scifi look into `src/theme/scifi.rs`: cyan token
  values, at least one theme-defined private token (proves the D6
  private-token path, e.g. the netlogo accent), the king video
  background as a tracked structural node (reserved low z band under
  `/window/content`), the first-run scramble splash as a tracked node +
  hide task gated on `is_first_time` (design D12), and the netstatus
  overlay fade as a tracked watcher on `is_visible` writing `alpha` via
  `ctx.set_touched` (remove the in-handler fade from the reconnect
  click handler in `src/app/schema/mod.rs`). Verify: with `theme=scifi`
  the app is pixel-comparable to today's look (video bg, splash on
  first run, cyan accents, fade on overlay open); `make compile-dev`
  and `make compile-apk`.

## 8. Theme setting and settings UI

- [ ] 8.1 Add the `theme` enum property to `create_setting` in
  `src/setting.rs` (items `minimal`, `scifi`; default `scifi`); fix
  `Setting::new` to skip-and-log persisted keys that no longer exist as
  node properties instead of unwrapping; guard reads so an unknown
  persisted theme falls back to the default. Verify persistence across
  restart and the fallback path (hand-edit the persisted value);
  `make compile-dev`.
- [ ] 8.2 Add enum rendering to the settings screen (`settings.rs`):
  cycle-on-tap row for enum values (starts with `theme`; also fixes
  `net.transport` showing "unknown"). Verify both enums display and
  modify correctly, and switching `theme` there drives the live switch;
  `make compile-dev`.

## 9. End-to-end verification

- [ ] 9.1 Full walkthrough under both themes (chat, menus, wallet send
  flow, receive, settings, netstatus overlay, emoji picker): minimal is
  complete and usable per the `app-theme` spec; scifi matches the
  current shipped look; live switch mid-session leaves no mixed state,
  no orphaned visuals, no running animations from the old theme; theme
  writes outside permitted masks are denied (spot-check a widget-owned
  prop via a debug theme write). Verify `make compile-dev` clean,
  `cargo test` in `bin/app` green, and `make compile-apk` for Android.
