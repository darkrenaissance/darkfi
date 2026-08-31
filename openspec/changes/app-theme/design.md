## Context

See proposal.md for motivation. The machinery this builds on, verified
against the code:

- `Property` (`src/prop/mod.rs`) resolves reads in layers: `vals[i]` →
  if it is an expression, `cache[i]` (else `defaults[i]`) → if unset,
  `defaults[i]` → type default. `defaults` is `Vec<PropertyValue>` and
  `PropertyValue::SExpr` exists, but defaults are only settable pre-`Arc`
  via builder `set_defaults_*` (no expression variant), so schema code
  can only write `vals`. `allow_exprs()` is also builder-only: a live
  property can never gain expr support, so factories must opt in.
- Exactly one evaluator exists: `PropertyRect::eval_with`
  (`src/prop/wrap.rs:377`) — gathers globals from `add_depend` edges plus
  extras (parent w/h), runs the s-expr machine per expr index, writes the
  cache. Widgets run it in their per-pass paths (`MultiLine::eval_rect`,
  etc.). Colors and other styled f32 props are always plain values today.
- Batching (`src/prop/guard.rs`): a `PropertyAtomicGuard` collects
  `(prop, role, action)` and on **Drop** notifies all subscribers with a
  shared `BatchGuard`. `RedrawTrigger::make_guard` (`src/ui/mod.rs:136`)
  binds the batch's `end_batch` to exactly one redraw token, fired when
  the last `BatchGuard` reference drops; `trigger()` enqueues into a
  bounded(1) channel, so multiple triggers coalesce into one pass.
- Pubsub (`src/pubsub.rs`): `Publisher::notify` is a non-blocking
  `try_send` into **unbounded** per-subscriber queues — events published
  before a listener task starts polling are buffered and drained later.
  Therefore applying a theme during `App::setup` (before widget listener
  tasks exist in `App::start`) is safe: nothing is lost.
- Widgets (`src/ui/text.rs:80-90, 231-264`) wrap their properties with
  `Role::Internal` and use `when_change_external` handlers that only
  clear the widget's draw cache and request a pass; the handler skips
  `Role::Internal` echoes so draw-pass evaluations don't retrigger
  passes forever. Evaluation belongs in the draw path; invalidation
  belongs in handlers.
- Node/task teardown precedent: `menu/mod.rs:486-509` channel deletion
  does `clear_tasks()` + `unlink()` + `redraw.trigger()`.
- Settings: `/setting` props persist via the `Setting` pimpl
  (`src/setting.rs`); `Setting::new` loads persisted rows with
  `get_property(&name).unwrap()` — an unknown persisted key panics.
  Enums render as "unknown" in the settings screen. Builder enum
  defaults are written as `PropertyValue::Str` (`set_defaults_str` on an
  Enum property), a latent type mismatch.
- Styling is inline across `src/app/schema/**` (23 files); the scifi
  palette leaks into `src/app/node.rs` factories;
  `COLOR_SCHEME`/`PaperLight` is a compile-time second look whose arms
  are dead (`const DarkMode`).

## Goals / Non-Goals

**Goals:**

- Layered theming that reuses property fallback semantics: baseline =
  defaults, theme = `vals`, unload = unset.
- Theme switching that is O(tokens), atomic, and covers nodes created at
  runtime (no re-theming pass, no `on_link` signal needed).
- A defined style/structure split: schema owns layout tree and functional
  wiring; themes own styling, decorations, and reactions.

**Non-Goals:**

- Making `VectorShape` internal colors expression-driven (shape colors
  are baked consts; scifi rebuilds themed shapes structurally — revisit
  later if a theme needs it).
- Dynamic/runtime-loaded themes; the registry is compile-time
  (`minimal`, `scifi`).
- Theming plugin-owned UI beyond what plugins inherit from shared tokens.
- A themes-from-disk or remote theme format.

## Decisions

### D1: Layer on defaults/vals instead of rebuilding or walking

Switching rebuilds nothing. Minimal installs defaults; themes set `vals`;
unload unsets. Alternatives considered: (a) destroy and re-run
`schema::make` per switch — loses runtime state (scroll, focus, joined
channels wiring) and is O(everything); (b) walk the tree setting props —
no `on_link` signal exists, so nodes created after a switch (new channel
screens) would be missed, and unload needs a journal of everything.
Defaults/vals gives restore semantics for free and pushes the dynamic-node
problem into creation-time wiring (D5).

### D2: Post-creation default installation, no modify events

New methods on `PropertyPtr` (`src/prop/mod.rs`), mutating `defaults[i]`
with the same type/length checks as the builder variants:

```rust
impl Property {
    pub fn set_default_bool(&self, i: usize, val: bool) -> Result<()>;
    pub fn set_default_u32(&self, i: usize, val: u32) -> Result<()>;
    pub fn set_default_f32(&self, i: usize, val: f32) -> Result<()>;
    pub fn set_default_f32_multi(&self, vals: &[f32]) -> Result<()>;
    pub fn set_default_str<S: Into<String>>(&self, i: usize, val: S) -> Result<()>;
    /// Writes PropertyValue::Enum (unlike builder set_defaults_str, which
    /// writes Str onto Enum properties — a latent type mismatch).
    pub fn set_default_enum<S: Into<String>>(&self, i: usize, val: S) -> Result<()>;
    pub fn set_default_expr(&self, i: usize, code: SExprCode) -> Result<()>;
    /// Raw variant used by the journal and token node construction.
    pub fn set_default_value(&self, i: usize, val: PropertyValue) -> Result<()>;
}
```

Installing a default emits **no** modification event: it is a
construction-time operation (before first frame) or happens inside a
switch batch where the accompanying unsets already notify. Rule:
defaults are never mutated as a live styling mechanism. Alternative — a
new `ModifyAction` for default changes — adds pubsub surface for no
current consumer; revisit if a future feature needs live default editing.

### D3: SExpr defaults with effective-source semantics; single shared cache

`is_expr`/`get_expr` resolve the **effective expression**: the `vals`
expression if present, else the `defaults` expression. `get_value`
resolution per the `prop-defaults` spec — never returns an unresolved
expression:

```rust
pub fn get_value(&self, i: usize) -> Result<PropertyValue> {
    match self.get_raw_value(i)? {
        PropertyValue::SExpr(_) => {
            let cached = self.get_cached(i)?;
            if !cached.is_null() { return Ok(cached) }
            // fall through to the default layer
            self.default_or_type_default(i)
        }
        PropertyValue::Unset => Ok(self.default_or_type_default(i)),
        v => Ok(v),
    }
}

/// default layer: plain default → default-expr cache → type default
fn default_or_type_default(&self, i: usize) -> PropertyValue {
    match &self.defaults[i] {
        PropertyValue::SExpr(_) => match self.get_cached(i) {
            Ok(c) if !c.is_null() => c,
            _ => self.typ.default_value(),
        },
        d => d.clone(),
    }
}
```

One cache per index is shared between the two expression sources — after
a source switch the cache is stale until the next evaluation. With
draw-side evaluation (D7) **every draw pass recomputes expr indices from
current dependency values**, so no pass can observe a stale cache: the
window closes at the next pass, which is exactly the pass the switch
triggers. Two separate caches would double the state for zero observable
gain. Alternative rejected.

### D4: Themes may override any property, including rect; ownership rule

Theme overrides are plain `vals` (value or expression) over baseline
default-exprs; unset restores. Exception — properties the owning widget
itself writes at runtime (e.g. multiline edit writes `rect[3]` height via
`Role::Internal` in `eval_rect`, `src/ui/edit/behave.rs:140`) are
last-writer-wins: themes MUST NOT override them, or the widget clobbers
the theme. This rule is **enforced** by D13: widget-written properties
carry write masks without `Theme`, so a theme attempt fails with
`PropertyPermissionDenied` instead of silently losing a write race. The
5.1 scrub assigns the masks; the D4 audit becomes that assignment.

### D5: Live dependency rewiring with automatic listener resync

`when_change` subscriptions snapshot `get_depends()` at widget
construction and the poll loop rebuilds its poll set from that list
every iteration (`ui/mod.rs:246-312`) — the list is only fixed because
it is captured by value. Instead of forbidding later wiring, make the
depends list a live thing:

1. `Property` (`src/prop/mod.rs`) gains a depends-changed publisher and
   a remover; `add_depend` notifies:

```rust
depends_pub: Publisher<()>,   // new field, beside on_modify

pub fn add_depend<S: Into<String>>(&self, prop: &PropertyPtr, i: usize, local_name: S) {
    self.depends.lock().unwrap().push(PropertyDepend { /* … */ });
    self.depends_pub.notify(());
}

/// Remove edges matching (dep prop, index, local name) — theme unload
/// restores original wiring with it.
pub fn remove_depend(&self, prop: &PropertyPtr, i: usize, local_name: &str);
```

2. `when_change_impl` (`src/ui/mod.rs`) shares the subscription list in
   an `Arc<Mutex<Vec<_>>>`, subscribes to `prop.depends_pub`, and on
   receipt rebuilds the dependency entries from a fresh `get_depends()`
   snapshot (entry 0 — the property itself — never changes), then runs
   the handler once more: dropping a receiver discards its queued
   messages, so the extra run closes any event missed during the swap.
   It is the existing invalidate+trigger handler, coalesced by the
   bounded(1) redraw channel.

Theme rules under this mechanism:

- Theme expressions may reference **any** property (not just
  creation-wired names) by adding depends at apply time; the resync
   makes the widget hear them. D4 still applies: never on properties
  the owning widget writes at runtime.
- Theme-added edges are journaled (D8) and removed on unload, so
  repeated switches don't accumulate stale edges.
- Theme-local names must be fresh (convention: prefixed, e.g.
  `th_*`) — a duplicate local name would shadow another in the eval
  globals.
- Startup ordering converges either way: if the theme applies before
  widget tasks start, the construction snapshot already includes the
  edges; if after, the buffered depends-changed event triggers resync.

The explicit `stop()`/`start()` walk (`win/mod.rs:276-291`) remains as a
coarse re-init fallback (rebuilds every handler of a widget); the theme
engine does not need it.

Alternatives considered: (a) creation-only wiring — rejected: too
restrictive for theme expressions; (b) explicit stop()/start() as the
primary mechanism — rejected: coarse (whole widget), async churn, and
the engine would have to locate affected widgets; kept as fallback.

### D6: `/theme` token node + wiring helpers ("classes")

A root-level `/theme` node (sibling of `/setting`) created **before**
`schema::make`, holding the themeable vocabulary as token properties:
colors as 4×f32 (`PropertySubType::Color`), sizes/spacing as f32. Token
defaults = minimal palette, installed at node construction (D2, no
events). Helpers replace inline styling blocks in schema code:

```rust
// src/theme/mod.rs (sketch)
/// Wire `prop_name` on `node` so each component follows the token
/// `token_name` on the /theme node. Installs default-exprs + depends.
/// Replaces the 4× set_f32 styling blocks used across the schema.
pub fn wire_color(
    node: &SceneNodePtr,
    prop_name: &str,
    theme: &SceneNodePtr,
    token_name: &str,
) -> Result<()> {
    let prop = node.get_property(prop_name).ok_or(Error::PropertyNotFound)?;
    let token = theme.get_property(token_name).ok_or(Error::PropertyNotFound)?;
    for i in 0..4 {
        let local = format!("{token_name}_{i}");
        prop.set_default_expr(i, expr::load_var(&local))?;
        prop.add_depend(&token, i, local);
    }
    Ok(())
}

/// Single-f32 variant for font_size, padding, spacing, etc.
pub fn wire_f32(
    node: &SceneNodePtr,
    prop_name: &str,
    theme: &SceneNodePtr,
    token_name: &str,
) -> Result<()> { /* same, index 0 */ }
```

The token is the class; the helper call is the class assignment. Theme
apply = set `vals` on ~30 tokens. **Prerequisite**: `allow_exprs()` is
builder-only, so every factory in `src/app/node.rs` must call it for
every themeable prop (`text_color`, `*_color`, `font_size`, `padding`,
`lineheight`, spacing…) — otherwise `set_default_expr`/`set_expr` fail
with `PropertySExprNotAllowed`. This factory pass happens with the Tier-0
scrub (tasks 5.1).

**Theme-defined tokens.** The `/theme` root carries only the *shared*
vocabulary — the tokens the schema wires defaults to, present in every
theme, forming the minimal palette. Themes are not boxed in by it:
`add_property` is builder-only (`&mut self` on an owned node), so live
properties cannot be appended to the linked `/theme` node — but child
nodes link to live parents freely (that is how the whole schema is
built). A theme therefore mints its private vocabulary as a tracked
child node:

```rust
// src/theme/mod.rs (sketch) — ThemeCtx helper
/// Create `/theme/<name>` carrying the theme's own token properties
/// (built pre-Arc with builder add_property + set_defaults_*), null
/// pimpl, linked under /theme and tracked for unload.
pub fn create_token_child(&self, props: Vec<Property>) -> Result<SceneNodePtr>;
```

Rules that fall out:

- `/theme` root = shared tokens, always present. Schema default-wiring
  (D6 helpers) may reference **only** these — a default must never
  dangle when its theme is inactive.
- `/theme/<name>` child = theme-private tokens (e.g. scifi's
  `glow_color`), present only while that theme is applied. They appear
  in theme-installed **vals** expressions and wired overrides
  (journaled, D5/D8) — never in schema defaults.
- Unload ordering matters: unset widget vals → remove journaled dep
  edges → unlink the token child. A dead dep edge makes
  `dep.prop.upgrade()` fail and eval error on every pass, so edges are
  removed before the node that holds their targets dies. (Defense in
  depth: eval may skip dead edges rather than error — implementer's
  choice, the ordering rule is the contract.)
- Token children are data-only nodes: null pimpl, nothing persisted
  (only the `theme` enum is), lookup-friendly for debugging
  (`/theme/scifi`).

### D7: Generalize evaluation from rect to bounded f32 arrays — in the draw path

Extract the `eval_with` pattern into a shared free function; add wrappers:

```rust
// src/prop/wrap.rs (sketch)
pub fn eval_f32_multi(
    prop: &PropertyPtr,
    atom: &mut PropertyAtomicGuard,
    role: Role,
    range: &[usize],
    extras: Vec<(String, f32)>,
) -> Result<()> {
    let mut globals = vec![];
    for dep in prop.get_depends() {
        let Some(dep_prop) = dep.prop.upgrade() else {
            return Err(Error::PropertyNotFound)
        };
        globals.push((dep.local_name, SExprVal::Float32(dep_prop.get_f32(dep.i)?)));
    }
    globals.extend(extras);
    let mut changes = vec![];
    for &i in range {
        if !prop.is_expr(i)? { continue }
        let expr = prop.get_expr(i)?;
        let mut machine = SExprMachine { globals: globals.clone(), stmts: &expr };
        changes.push((i, machine.call()?.as_f32()?));
    }
    prop.set_cache_f32_multi(atom, role, changes).unwrap();
    Ok(())
}

impl PropertyColor {
    /// Globals only (no w/h extras). Called at the top of draw().
    pub fn eval(&self, atom: &mut PropertyAtomicGuard) -> Result<()> {
        eval_f32_multi(self.prop(), atom, self.role, &[0, 1, 2, 3], vec![])
    }
}
```

`PropertyRect::eval_with` becomes a thin wrapper (extras = parent w/h).
`PropertyFloat32::eval` serves `font_size`/spacing (index 0).

**Call sites are in the draw path, not in when_change handlers** — this
is the same pattern rects already use, and it is what makes evaluation
timing irrelevant: the pass triggered by a switch recomputes every expr
index from current token values before reading them. Handlers stay
invalidation-only (`draw_cache.clear(); redraw.trigger()` — the existing
`text.rs:257-260` shape). The cache writes use the widget wraps'
`Role::Internal`, so `when_change_external` skips them as eval echoes,
exactly like rect evals today.

### D8: Theme engine with tracked nodes, tasks, and a journal

`src/theme/`: trait, compile-time registry, and a per-application ctx:

```rust
pub trait Theme: Send + Sync {
    fn name(&self) -> &'static str;
    fn apply<'a>(&'a self, ctx: &'a ThemeCtx) -> BoxFuture<'a, Result<()>>;
}

/// Unload bookkeeping. Nodes are the storage: theme-created properties
/// live on tracked nodes, theme tasks are pushed onto tracked nodes
/// (SceneNode::push_task) and cancel with clear_tasks(); the ctx only
/// records what cannot be reconstructed.
///
/// The touched-set is required, not an optimization: `vals` carries no
/// authorship, so a theme override is indistinguishable from schema
/// setup or runtime state (scroll, typed text, is_visible) — unloading
/// by deep-walking and resetting every property would destroy runtime
/// state. Unload therefore resets exactly the (prop, i) pairs the
/// theme recorded, which also means the ctx is the ONLY sanctioned way
/// for a theme to modify properties it does not own.
pub enum JournalEntry {
    /// Bounded `vals` override — unload unsets it (falls to default).
    /// No prior value needed: defaults ARE the baseline.
    Touched { prop: PropertyPtr, i: usize },
    /// Unbounded/direct-list override (e.g. nick_colors) — unbounded
    /// props have no defaults tier, so the prior entries are recorded.
    List { prop: PropertyPtr, prior: Vec<PropertyValue> },
    /// Theme-added dependency edge (D5), removed on unload.
    Depend { prop: PropertyPtr, dep_prop: PropertyPtr, i: usize, local_name: String },
}

/// What a theme (and the engine) may do; everything done through the
/// ctx is undone by unload.
pub struct ThemeCtx {
    app: AppPtr,
    /// Roots the theme linked into pre-existing (schema) trees —
    /// including its `/theme/<name>` token child. Descendants ride
    /// along: unlink drops the subtree.
    nodes: SyncMutex<Vec<SceneNodePtr>>,
    journal: SyncMutex<Vec<JournalEntry>>,
}

impl ThemeCtx {
    /// Link a theme-owned node under an existing parent; unlinked on
    /// unload (clear_tasks() cancels its tasks, unlink() drops the
    /// subtree).
    pub fn link_tracked(&self, parent: &SceneNodePtr, child: SceneNodePtr);
    /// Build `/theme/<name>` carrying theme-defined token properties
    /// (D6); linked, tracked, data-only — also the home for theme
    /// tasks (created lazily for themes with no other nodes).
    pub fn create_token_child(&self, name: &str, props: Vec<Property>) -> Result<SceneNodePtr>;
    /// Push a task onto a theme-owned node so teardown cancels it.
    pub fn push_task(&self, node: &SceneNodePtr, task: smol::Task<()>);
    /// Set a bounded property value (stamped Role::Theme) — journal
    /// records only (prop, i); dedups repeat entries (e.g. per-step
    /// animation writes).
    pub fn set_touched(&self, atom: &mut PropertyAtomicGuard, prop: &PropertyPtr,
                       i: usize, val: PropertyValue) -> Result<()>;
    /// Overwrite an unbounded list (stamped Role::Theme) — journal
    /// records the prior entries.
    pub fn set_list(&self, atom: &mut PropertyAtomicGuard, prop: &PropertyPtr,
                    vals: Vec<PropertyValue>) -> Result<()>;
    /// Add a dependency edge, recording it for removal (D5).
    pub fn depend_journaled(&self, prop: &PropertyPtr, dep: &PropertyPtr,
                            i: usize, local_name: String);
}

pub fn registry_lookup(name: &str) -> Option<&'static dyn Theme>;
```

`minimal` is the registry's identity element: it has no `apply`
implementation (or an empty one) — the minimal look *is* the unloaded
state (token defaults + schema defaults), so switching to minimal is
just unload. Theme **behavior** is property watchers — scifi's fade:

```rust
// src/theme/scifi.rs (sketch)
async fn apply(ctx: &ThemeCtx) -> Result<()> {
    let tokens = ctx.create_token_child("scifi", private_token_props())?;
    set_tokens(ctx, &[("accent_color", [0., 0.94, 1., 1.]), /* … */]).await?;
    king_video_node(ctx).await?;                 // tracked structural node
    if ctx.app.is_first_time.load(Ordering::Relaxed) {
        scramble_splash(ctx).await?;             // tracked node + hide task (D12)
    }

    // Watch the overlay toggle (existing property, existing pubsub) and
    // animate alpha. Replaces the fade inside the reconnect click
    // handler in src/app/schema/mod.rs. The watcher is a task pushed
    // onto the theme's token child — cancelled with clear_tasks() when
    // that node is unlinked (tasks live on nodes, D8).
    let overlay = ctx.app.sg_root.lookup_node("/window/content/chat/netstatus_overlay").unwrap();
    let is_visible = overlay.get_property("is_visible").unwrap();
    let alpha = overlay.get_property("alpha").unwrap();
    let sub = is_visible.subscribe_modify();
    let ex = ctx.app.ex.clone();
    let fade_task = ex.spawn(async move {
        loop {
            let Ok((_, _, guard)) = sub.receive().await else { break };
            if !is_visible.get_bool(0).unwrap() { continue }
            for step in 1..=50 {
                msleep(20).await;
                let atom = &mut guard.spawn();
                // Role::Theme via ctx; touched-set entry dedups across steps
                ctx.set_touched(atom, &alpha, 0, PropertyValue::Float32(step as f32 / 50.))?;
            }
        }
    });
    ctx.push_task(&tokens, fade_task);
    Ok(())
}
```

The engine (not the trait) performs unload from the ctx state, so themes
cannot leak by forgetting cleanup. Unload order is part of the contract:
`clear_tasks()` on tracked nodes **first** (cancel the actors — an
in-flight watcher writing `alpha` after its unset would leave a stale
value nothing resets) → unset `Touched` entries → restore `List` entries
→ remove `Depend` edges → `clear_values` on shared tokens → `unlink()`
per tracked node (drops subtrees and their properties; token children
included). Edges come off before the nodes holding their targets die, so
no eval ever sees a dangling dep (D6).

**`Role::Theme`.** `vals` carries no authorship, so attribution lives on
the event stream: a new `Role::Theme` variant, stamped by every ctx
setter (`set_touched`, `set_list`, shared/private token sets). Consumer
audit: `when_change_impl` filters by equality (`ui/mod.rs:279-281`,
skipping only `Internal`/`Ignored`), so `Theme` events reach widgets
exactly like `App` ones — no filter change, but verified during
implementation; `net.rs`/`setting.rs` produce roles rather than match
them, and any external Role mapping needs a `Theme` arm. Role is
bin/app-local (not serialized), so no wire impact. Bonus this buys: a
dev-mode leak audit — a `Role::Theme` event arriving on a (prop, i) not
in the touched-set means something bypassed the ctx; warn-log it. The
ctx setters take no role parameter; `Role::Theme` is not caller-choice.

### D9: Atomic switch flow — real guard mechanics

One batch for the whole switch; structural changes get an explicit
trigger because unlinking notifies no properties (same as `menu/mod.rs`):

```rust
// src/theme/mod.rs (sketch)
pub async fn switch(app: &App, current: &mut InstalledTheme, next_name: &str) -> Result<()> {
    let ctx = ThemeCtx::new(app);
    {
        let atom = &mut app.redraw_trigger.make_guard(gfxtag!("theme switch"));

        // Unload: clear_tasks (cancel actors) → unset Touched →
        // restore Lists → remove Depend edges → unset shared token
        // vals → unlink tracked nodes
        unload_current(current, &ctx, atom);

        // Load: next theme's tokens/nodes/watchers
        let next = registry_lookup(next_name)
            .or_else(|| registry_lookup(DEFAULT_THEME))
            .ok_or(Error::ThemeNotFound)?;
        if let Some(apply) = next.apply_fn() { apply(&ctx).await?; }
        *current = InstalledTheme::from(next, ctx);
    } // guard Drop: notify all queued actions under one BatchGuard id

    // Unlinks changed the tree but no properties; request a pass
    // explicitly (coalesced by the bounded(1) channel — free if the
    // batch already triggered one).
    app.redraw_trigger.trigger();
    Ok(())
}
```

What "atomic" concretely means here, per `guard.rs`/`ui/mod.rs`:

1. All unload+apply property actions are queued in one guard and
   notified together at Drop under a single `BatchGuardId` — no listener
   ever sees a half-switched batch.
2. Widget handlers that react clear draw caches and enqueue triggers;
   the bounded(1) redraw channel coalesces every trigger into one pass.
3. That pass re-evaluates all expr indices draw-side (D7) from the
   already-settled token values, so the frame is internally consistent
   by construction — mixed state cannot be drawn, not merely unlikely.

Startup application and live switching share this function; the
`/setting/theme` watcher is an **engine-owned** task (not theme-tracked,
never unloaded with a theme).

### D10: Setting integration and robustness

`theme` enum property in `create_setting` (items `minimal`, `scifi`;
default `scifi` installed via D2's `set_default_enum` — Enum variant,
not Str). Persistence is free via the `Setting` pimpl. Read-side guard:
unknown persisted theme → default (spec scenario). Fix `Setting::new`
to **skip** persisted keys that no longer exist as node properties
instead of `unwrap()`-panicking — a pre-existing latent bug this change
makes reachable (persisted `theme` vs. older binary, or a renamed
setting later). Settings screen gains a minimal enum control
(cycle-on-tap row); theme names are shown raw (no i18n) in v1, matching
existing settings rows.

### D11: Sequencing — PaperLight dies first

All 14 PaperLight-bearing files are reworked again by the split; deleting
the dead scheme first halves the branch noise and makes the minimal/scifi
diff reviewable. Mechanical collapse: `match COLOR_SCHEME` arms inline to
their DarkMode values; `ColorScheme`/`COLOR_SCHEME` and the window-linked
`bg` vector-art block are deleted.

### D12: The splash is scifi-owned

The first-run scramble splash is theme content: the scramble effect and
its colors are scifi flavor, not baseline structure. It moves out of
`schema::make` into scifi's `apply` as a tracked node plus a tracked
hide task, gated on `is_first_time` (readable from the app context at
apply time, before it is consumed in `App::start`). Consequence: the
minimal theme has no splash — first run under minimal goes straight to
the baseline UI, which the completeness requirement already covers.

### D13: Property permissions — Role bitflags + read/write masks

`Role` becomes a bitflag set and properties carry a permission pair;
setters/getters check the acting role against the mask and fail with a
new `Error::PropertyPermissionDenied`:

```rust
// src/prop/mod.rs (sketch) — hand-rolled u8 bit ops, no new dependency
pub struct Role: u8 {
    const User     = 1 << 0;
    const App      = 1 << 1;
    const Internal = 1 << 2;
    const Ignored  = 1 << 3;   // marker ("don't notify"), rarely in masks
    const Theme    = 1 << 4;
}

pub struct PropertyPermission {
    /// Roles allowed to read.
    pub read: Role,
    /// Roles allowed to write (set/unset/push/insert/remove/clear/expr).
    pub write: Role,
}

impl Property {
    pub fn new<S: Into<String>>(name: S, typ: PropertyType, subtype: PropertySubType,
                                permission: PropertyPermission) -> Self;
    pub fn can_read(&self, role: Role) -> bool;
    pub fn can_write(&self, role: Role) -> bool;
}
```

Hand-rolled bit ops rather than the `bitflags` crate: `bitflags` is not
a dependency of `bin/app` today, and adding one is a supply-chain
decision requiring human review (repo rules) — for a two-flag type the
newtype is trivial.

**Write enforcement is centralized and immediate**: every mutating API
already carries a role (`set_*`, `set_expr`, `unset`, `clear_values`,
`push_*`, `insert_*`, `remove_*`) — a single `can_write` check before
any mutation or journal entry; denial returns
`PropertyPermissionDenied` with the property untouched. This makes two
existing conventions *enforced invariants*: D4 (themes cannot override
widget-written properties — the write mask simply lacks `Theme`) and
the ctx-only rule (a theme writing outside `set_touched` still stamps
`Role::Theme` and is denied at the property layer if the mask forbids
it).

**Read enforcement is staged by where an actor is attributable** — raw
`get_*` calls carry no role (145 sites) and stay trusted in-crate for
now:

1. Wrap layer: every widget reads through `Property{Float32,Color,…}`
   wraps, which already hold a role from `wrap()` — `get()` checks
   `can_read`, and `wrap()` itself validates the role upfront (failing
   construction instead of at first read).
2. Expr evaluation: `eval_f32_multi` reads dependencies on behalf of a
   wrap role — dep reads go through the check.
3. External boundary: `net.rs` RPC property reads check against the
   remote actor's role.

Full role parameters on raw getters is future hardening, deliberately
not in this change (each of the 145 sites would need a reasoned role,
not a mechanical one).

**Exemptions** (by design): `set_default_*` (construction metadata, D2),
`set_cache_*` (derived eval artifacts, written `Internal`), and
`add_depend` (wiring metadata). Modifying a default still cannot be
gated by write masks — defaults are installed before the tree is live.

**Factory masks make the style/structure/function split concrete**
(assigned during the 5.1 scrub; transitional default
`PropertyPermission { read: all, write: all }` keeps current behavior
until then):

```
prop class                     read                write
─────────────────────────────  ──────────────────  ────────────────────
themeable style (colors,       all internal roles  App | Theme
font_size, padding, spacing)
widget-computed/runtime        all internal roles  Internal (| App for
(scroll, is_focused,                               schema bootstrap)
height, select_text, alpha
on multiline rect[3])
behavior toggles               all internal roles  App (never Theme —
(is_active, is_visible)                            themes don't change
                                                   what the UI does)
content/data (text, items,     all internal roles  App (User where
nick_colors)                                       user input persists)
settings (/setting/*)          all                 User | App
tokens (/theme/*)              all                 Theme (set at
                                                   apply, unset at
                                                   unload — nobody
                                                   else writes them)
```

`Role::Theme` stamping (D8) and this table are two views of the same
contract: the mask says who may, the role on the event says who did.

## Structural changes

```
bin/app/src/
├── prop/mod.rs        + set_default_{bool,u32,f32,str,enum,expr,value}
│                        (post-Arc, D2); effective is_expr/get_expr/get_value (D3);
│                        builder set_defaults_expr; Role bitflags +
│                        PropertyPermission on Property::new + can_read/
│                        can_write + write-path enforcement (D13); unit tests
├── error.rs           + Error::PropertyPermissionDenied (D13)
├── prop/wrap.rs       + eval_f32_multi free fn; PropertyColor::eval;
│                        PropertyFloat32::eval; PropertyRect::eval_with delegates (D7)
├── app/node.rs        factories: allow_exprs() on themeable props + Tier-0 scrub (D6)
├── theme/mod.rs   NEW Theme trait, ThemeCtx (nodes-as-storage: touched-set,
│                        list/dep journal, token children, tasks on nodes),
│                        registry, switch(), apply_startup(),
│                        create_token_node(), create_token_child(),
│                        wire_color/wire_f32 (D6-D9)
├── theme/scifi.rs NEW scifi impl: tokens, king video, splash, fade watcher (D8, D12)
├── app/mod.rs         App::setup: link /theme before schema::make; apply_startup after
├── setting.rs         theme enum prop; skip unknown persisted keys (D10)
├── app/schema/**      PaperLight removal; Tier-1 defaults + wiring helpers (D1, D11)
└── app/schema/settings.rs  enum rendering (cycle-on-tap) (D10)
```

`minimal` has no file: it is the unloaded state.

## Flows

### Setup and startup application

```
main.rs
 └─ App::setup(kv_db, app_db)
     ├─ link /setting      Setting pimpl loads persisted props synchronously
     │                     (incl. `theme`; unknown keys skipped — D10)
     ├─ link /window
     ├─ link /theme        NEW: token props constructed with minimal
     │                     palette as defaults (D2, no events)
     ├─ schema::make(...)  structure + Tier-1 defaults + wire_color/wire_f32
     │                     (helpers read /theme; all depends wired here — D5)
     └─ theme::apply_startup()
         ├─ read /setting/theme → "scifi" (unknown → default — D10)
         ├─ switch(app, ∅ → scifi)                       (D9)
         │   └─ scifi tokens set → notifications queue (unbounded — safe,
         │      widget listener tasks don't exist yet; they drain later)
         └─ spawn engine-owned /setting/theme watcher (live switches)

 App::start(event_pub, epoch)
 ├─ window.init(); redraw_trigger.trigger()     (first pass queued)
 └─ start_procs → Window::start                 draw loop + widget OnModify
                                                tasks start; queued token
                                                notifications drain into
                                                handlers (cache clears)
      └─ first draw pass: every widget re-evals its expr-bound props
         (rect + colors + fonts — D7) from settled scifi token values
         → first frame is fully scifi (spec: applied before first frame)
```

### Live switch (scifi → minimal)

```
user taps theme row in settings screen
 └─ setting prop `theme` set → Setting pimpl persists; engine watcher fires
     └─ switch(app, scifi → minimal)
         ┌──────────────────────────────────────────────────────────┐
         │ atom = redraw.make_guard("theme switch")                  │
         │                                                            │
         │ UNLOAD scifi (journal replay in reverse + teardown):        │
         │ UNLOAD scifi (cancel actors, then unwind):                  │
         │   clear_tasks() on tracked nodes FIRST (in-flight fade      │
         │     stops before it can write stale vals)                   │
         │   unset Touched entries → vals fall to schema defaults      │
         │   restore List entries (nick_colors priors)                 │
         │   remove Depend edges (incl. private-token refs)           │
         │   shared token.clear_values() × ~30  (Unset, queued)       │
         │   unlink() tracked nodes — /theme/scifi dies with its      │
         │     props and tasks, king video, splash                    │
         │                                                            │
         │ LOAD minimal: registry identity element — nothing to do   │
         └──────────────────────────────────────────────────────────┘
         drop(atom) ─▶ one notification wave (single BatchGuardId)
                         │
                         ├─ widget handlers: draw_cache.clear() + trigger()
                         │  (bounded(1) channel coalesces all triggers)
                         └─ last BatchGuard ref drops ─▶ one redraw token
                                                        (plus the explicit
                                                        trigger for unlinks)
                         │
                         ▼
         draw pass: draw-side eval reads token *defaults* (= minimal
         palette) → one consistent minimal frame; scifi nodes absent
         from the tree; no residue (spec: atomic switching, teardown)
```

### Redraw guarantee (why no mixed frame is possible)

- Property mutations are only observed through the draw pass; a pass
  only starts by draining a token from the bounded(1) channel, and tokens
  are enqueued after state is settled (`make_guard` defers to end-of-
  batch; handlers mutate then trigger).
- Passes re-evaluate every expr index from current dependency values
  before reading them (D7) — a pass cannot draw a stale cache even if it
  races a handler.
- Structural-only changes (unlink) notify nothing, hence the explicit
  `trigger()` in `switch` — mirroring the `menu/mod.rs` precedent.

## Risks / Trade-offs

- [Theme overrides on widget-written properties get clobbered] → D4
  ownership rule, audited per widget during the schema split; enforcement
  metadata deferred.
- [Live dependency rewiring touches the widget hot path
  (`when_change_impl`)] → D5 keeps the existing poll loop; resync is
  rare (theme switches), adds one subscription per watched prop, and
  the post-resync handler run is the existing coalesced
  invalidate+trigger.
- [Draw-side eval adds per-pass cost] → same order as today's rect
  evals; exprs are tiny (var loads + arithmetic); only wired (themeable)
  props carry exprs.
- [`Setting::new` panics on unknown persisted keys] → fixed as part of
  D10 (skip-and-log), before the new `theme` key ships.
- [Theme node z-order collisions with schema layers] → theme-owned
  background/decoration nodes take a reserved low band (`z_index` 0 under
  `/window/content`); convention documented with the engine.
- [Volume: 23 schema files to convert] → helpers land first, then
  per-area conversion (menu, chat, wallet, settings, root); PaperLight
  removal (D11) precedes; visual check per area under both themes.
- [VectorArt shape colors not tokenizable] → scifi rebuilds its themed
  shapes structurally; neutral shapes keep baked colors (non-goal).
- [First-frame ordering: theme applied before widget tasks exist] → safe
  by construction: pubsub queues are unbounded (events buffer), and the
  first pass evaluates draw-side from settled token values (Flows).

## Migration Plan

Single change, no flag: default `scifi` means the shipped look is
unchanged on upgrade — the new `/setting/theme` key is the only persisted
addition, and the D10 skip-guard makes it harmless to older binaries.
Rollback is revert; any persisted `theme` value is ignored (skipped)
by pre-change code once D10's guard is in.

Implementation order is the tasks order: PaperLight removal → property
APIs → evaluation → tokens/wiring → schema split → engine → scifi →
setting/UI. Each step keeps `make compile-dev` green; behavior
verification per area under both themes.

## Open Questions

- Exact shared token vocabulary (`accent_color` vs split accent/dim/
  accent2, how many `edit.*`/`menu.*`/`chatview.*` tokens): grown by
  trial and error during the split; does not affect the mechanism. The
  pressure is lower than it first looks — only *schema-referenced*
  tokens must exist up front; themes mint anything else privately (D6).
