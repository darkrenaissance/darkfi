/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! Runtime theme system: the `/theme` shared token node, class-wiring
//! helpers, and (see `engine` internals brought in by later tasks) the
//! theme lifecycle.
//!
//! Style/structure split: schema code owns the layout tree and wires
//! styled properties onto shared tokens via [`wire_color`] /
//! [`wire_f32`]; themes own styling by setting token `vals`. The token
//! defaults ARE the neutral `minimal` palette, so the unloaded state is
//! a complete look (design D1/D6).

use parking_lot::Mutex as SyncMutex;
use std::sync::Arc;

use tracing::{error, info};

use crate::{
    app::App,
    error::{Error, Result},
    expr,
    gfx::gfxtag,
    prop::{
        Property, PropertyAtomicGuard, PropertyPermission, PropertyPtr, PropertySubType,
        PropertyType, PropertyValue, Role,
    },
    pubsub::Subscription,
    scene::{Pimpl, SceneNode, SceneNodePtr, SceneNodeType},
    ui::{get_ui_object3, get_ui_object_ptr, RedrawTrigger},
};

pub mod scifi;

/// Every token a theme may set, and the value it falls back to when no
/// theme is loaded. Writing tokens is restricted to `Role::Theme`:
/// themes set them at apply time and unset them at unload; nobody else
/// writes them (design D13).
fn color_prop(name: &str, val: [f32; 4]) -> Property {
    let mut prop = Property::new(
        name,
        PropertyType::Float32,
        PropertySubType::Color,
        PropertyPermission { read: Role::ALL, write: Role::Theme },
    );
    prop.set_array_len(4);
    prop.set_defaults_f32(val.to_vec()).unwrap();
    prop
}

fn f32_prop(name: &str, val: f32) -> Property {
    let mut prop = Property::new(
        name,
        PropertyType::Float32,
        PropertySubType::Pixel,
        PropertyPermission { read: Role::ALL, write: Role::Theme },
    );
    prop.set_defaults_f32(vec![val]).unwrap();
    prop
}

/// The shared token vocabulary with the `minimal` palette as defaults.
/// Schema default-wiring may reference ONLY these tokens — a default
/// must never dangle when its theme is inactive (design D6). Themes are
/// not boxed in by this list: they mint private tokens as tracked child
/// nodes under `/theme/<name>` (engine task).
pub fn shared_token_props() -> Vec<Property> {
    vec![
        // Generic surfaces
        color_prop("text_color", [0.92, 0.92, 0.92, 1.]),
        color_prop("text_dim_color", [0.62, 0.62, 0.62, 1.]),
        color_prop("bg_color", [0.07, 0.07, 0.07, 1.]),
        color_prop("bg_dim_color", [0.04, 0.04, 0.04, 1.]),
        color_prop("bg_overlay_color", [0.03, 0.03, 0.03, 0.92]),
        color_prop("accent_color", [0.75, 0.75, 0.75, 1.]),
        color_prop("sep_color", [0.28, 0.28, 0.28, 1.]),
        // Edit widget
        color_prop("edit.text_color", [0.92, 0.92, 0.92, 1.]),
        color_prop("edit.bg_color", [0.10, 0.10, 0.10, 1.]),
        color_prop("edit.hi_bg_color", [0.35, 0.35, 0.35, 1.]),
        color_prop("edit.text_hi_color", [0., 0., 0., 1.]),
        color_prop("edit.cursor_color", [0.90, 0.90, 0.90, 1.]),
        color_prop("edit.placeholder_color", [0.50, 0.50, 0.50, 1.]),
        color_prop("edit.action_fg_color", [0.90, 0.90, 0.90, 1.]),
        color_prop("edit.action_bg_color", [0.15, 0.15, 0.15, 1.]),
        // Menu widget
        color_prop("menu.bg_color", [0.05, 0.05, 0.05, 0.5]),
        color_prop("menu.sep_color", [0.28, 0.28, 0.28, 1.]),
        color_prop("menu.role1_color", [0.60, 0.60, 0.60, 1.]),
        color_prop("menu.role2_color", [0.75, 0.75, 0.75, 1.]),
        // ChatView
        color_prop("chatview.bg_color", [0.02, 0.02, 0.02, 1.]),
        color_prop("chatview.timestamp_color", [0.55, 0.55, 0.55, 1.]),
        color_prop("chatview.text_color", [0.92, 0.92, 0.92, 1.]),
        color_prop("chatview.hi_bg_color", [0.25, 0.25, 0.25, 1.]),
        color_prop("chatview.action_text_color", [0.80, 0.80, 0.80, 1.]),
        color_prop("chatview.url_text_color", [0.70, 0.80, 0.90, 1.]),
        // Typography / spacing
        f32_prop("font_size", 18.),
        f32_prop("message_spacing", 8.),
        f32_prop("line_height", 1.2),
    ]
}

/// Build the `/theme` node carrying the shared token vocabulary with
/// the `minimal` palette as defaults. Data-only (null pimpl); linked at
/// the scene root beside `/setting`, BEFORE `schema::make` so schema
/// wiring can depend on it.
pub fn create_theme_node() -> SceneNodePtr {
    let mut node = SceneNode::new("theme", SceneNodeType::Object);
    for prop in shared_token_props() {
        node.add_property(prop).unwrap();
    }
    node.setup_null()
}

/// Convenience lookup of the linked `/theme` node.
pub fn get_theme_node(sg_root: &SceneNodePtr) -> Result<SceneNodePtr> {
    let node = sg_root.lookup_node("/theme").ok_or_else(|| Error::NodeNotFound)?;
    Ok(node)
}

/// Wire `prop_name` on `node` so each of its four components follows the
/// token `token_name` on the `theme` node: installs default-exprs (the
/// minimal-palette fallback layer) plus one dependency edge per
/// component. One wiring, and the widget tracks the token forever —
/// including nodes created at runtime (design D5/D6).
pub fn wire_color(
    node: &SceneNode,
    prop_name: &str,
    theme: &SceneNode,
    token_name: &str,
) -> Result<()> {
    let prop = node.get_property(prop_name).ok_or(Error::PropertyNotFound)?;
    let token = theme.get_property(token_name).ok_or(Error::PropertyNotFound)?;
    for i in 0..4 {
        let local = format!("{token_name}_{i}");
        prop.set_default_expr(i, expr::load_var(&local))?;
        prop.add_depend(Role::App, &token, i, local);
    }
    Ok(())
}

/// Single-f32 variant of [`wire_color`] for `font_size`, spacing, etc.
pub fn wire_f32(
    node: &SceneNode,
    prop_name: &str,
    theme: &SceneNode,
    token_name: &str,
) -> Result<()> {
    let prop = node.get_property(prop_name).ok_or(Error::PropertyNotFound)?;
    let token = theme.get_property(token_name).ok_or(Error::PropertyNotFound)?;
    let local = token_name.to_string();
    prop.set_default_expr(0, expr::load_var(&local))?;
    prop.add_depend(Role::App, &token, 0, local);
    Ok(())
}

// ============================================================================
// Theme engine (design D8/D9)
// ============================================================================

/// Unload bookkeeping. Nodes are the storage: theme-created properties
/// live on tracked nodes, theme tasks are pushed onto tracked nodes and
/// cancel with `clear_tasks()`; the journal records only what cannot be
/// reconstructed.
///
/// The touched-set is required, not an optimization: `vals` carries no
/// authorship, so a theme override is indistinguishable from schema
/// setup or runtime state (scroll, typed text, is_visible). Unloading
/// by deep-walking and resetting every property would destroy runtime
/// state; it resets exactly the (prop, i) pairs recorded here.
#[derive(Clone)]
pub enum JournalEntry {
    /// Bounded `vals` override — unload unsets it (falls to default).
    /// No prior value needed: defaults ARE the baseline.
    Touched { prop: PropertyPtr, i: usize },
    /// Theme-added dependency edge (D5), removed on unload.
    Depend { prop: PropertyPtr, dep_prop: PropertyPtr, i: usize, local_name: String },
}

/// What a theme (and the engine) may do; everything done through the
/// ctx is undone by unload.
pub struct ThemeCtx {
    sg_root: SceneNodePtr,
    redraw: RedrawTrigger,
    app: Option<Arc<App>>,
    /// Roots the theme linked into pre-existing (schema) trees —
    /// including its `/theme/<name>` token child. Descendants ride
    /// along: unlink drops the subtree.
    nodes: Arc<SyncMutex<Vec<SceneNodePtr>>>,
    journal: Arc<SyncMutex<Vec<JournalEntry>>>,
}

impl ThemeCtx {
    /// Full-application constructor.
    pub fn from_app(app: &Arc<App>) -> Self {
        Self {
            sg_root: app.sg_root.clone(),
            redraw: app.redraw_trigger.clone(),
            app: Some(app.clone()),
            nodes: Arc::new(SyncMutex::new(vec![])),
            journal: Arc::new(SyncMutex::new(vec![])),
        }
    }

    /// Parts constructor (also used by unit tests, which have no
    /// renderer/App).
    pub fn new_parts(sg_root: SceneNodePtr, redraw: RedrawTrigger) -> Self {
        Self {
            sg_root,
            redraw,
            app: None,
            nodes: Arc::new(SyncMutex::new(vec![])),
            journal: Arc::new(SyncMutex::new(vec![])),
        }
    }

    fn new(app: &Arc<App>) -> Self {
        Self::from_app(app)
    }

    /// The application handle, when the ctx was built from one.
    pub fn app(&self) -> Option<&Arc<App>> {
        self.app.as_ref()
    }

    /// The scene root.
    pub fn sg_root(&self) -> &SceneNodePtr {
        &self.sg_root
    }

    /// The redraw trigger (for watcher tasks building their own guards).
    pub fn redraw(&self) -> &RedrawTrigger {
        &self.redraw
    }

    /// A shallow handle sharing the same journal/node tracking — used by
    /// theme watcher tasks that must keep writing through the ctx.
    pub fn clone_shallow(&self) -> Self {
        Self {
            sg_root: self.sg_root.clone(),
            redraw: self.redraw.clone(),
            app: self.app.clone(),
            nodes: self.nodes.clone(),
            journal: self.journal.clone(),
        }
    }

    /// Link a theme-owned node under an existing parent; unlinked on
    /// unload (`clear_tasks()` cancels its tasks, `unlink()` drops the
    /// subtree).
    pub fn link_tracked(&self, parent: &SceneNodePtr, child: SceneNodePtr) {
        parent.link(child.clone());
        // Match the startup lifecycle: Window/Layer::init() and ::start()
        // run once at App::start and never again, so nodes linked at
        // runtime would sit dead — pimpls that load resources in init()
        // (Video::load_video) or register when_change handlers in
        // start() would never run them. Null pimpls (data-only token
        // nodes) have no lifecycle to run.
        if !matches!(child.pimpl(), Pimpl::Null) {
            let obj = get_ui_object3(&child);
            obj.init();
            if let Some(app) = &self.app {
                let ex = app.ex.clone();
                let ex2 = ex.clone();
                let child2 = child.clone();
                let task = ex.spawn(async move {
                    get_ui_object_ptr(&child2).start(ex2).await;
                });
                child.push_task(task);
            }
        }
        self.nodes.lock().push(child);
    }

    /// Build `/theme/<name>` carrying theme-defined token properties
    /// (constructed pre-Arc with builder `add_property`), linked under
    /// `/theme`, tracked, data-only (null pimpl). Private tokens appear
    /// only in theme-installed vals expressions and journaled overrides
    /// — never in schema defaults.
    pub fn create_token_child(&self, name: &str, props: Vec<Property>) -> Result<SceneNodePtr> {
        let parent = self.sg_root.lookup_node("/theme").ok_or(Error::NodeNotFound)?;
        let mut node = SceneNode::new(name, SceneNodeType::Object);
        for prop in props {
            node.add_property(prop).unwrap();
        }
        let node = node.setup_null();
        self.link_tracked(&parent, node.clone());
        Ok(node)
    }

    /// Push a task onto a theme-owned node so teardown cancels it.
    pub fn push_task(&self, node: &SceneNodePtr, task: smol::Task<()>) {
        node.push_task(task);
    }

    /// Set a bounded property value (stamped `Role::Theme` — attribution
    /// lives on the event stream since `vals` has none). The journal
    /// records only (prop, i); repeat entries (per-step animation
    /// writes) dedup.
    pub fn set_touched(
        &self,
        atom: &mut PropertyAtomicGuard,
        prop: &PropertyPtr,
        i: usize,
        val: PropertyValue,
    ) -> Result<()> {
        prop.set_value(atom, Role::Theme, i, val)?;
        let mut journal = self.journal.lock();
        if !journal.iter().any(|e| matches!(e, JournalEntry::Touched { prop: p, i: j } if Arc::ptr_eq(p, prop) && *j == i))
        {
            journal.push(JournalEntry::Touched { prop: prop.clone(), i });
        }
        Ok(())
    }

    /// Set all four components of a color property (see `set_touched`).
    pub fn set_touched_color(
        &self,
        atom: &mut PropertyAtomicGuard,
        prop: &PropertyPtr,
        val: [f32; 4],
    ) -> Result<()> {
        for (i, v) in val.into_iter().enumerate() {
            self.set_touched(atom, prop, i, PropertyValue::Float32(v))?;
        }
        Ok(())
    }

    /// Set a single-f32 property value (see `set_touched`).
    pub fn set_touched_f32(
        &self,
        atom: &mut PropertyAtomicGuard,
        prop: &PropertyPtr,
        val: f32,
    ) -> Result<()> {
        self.set_touched(atom, prop, 0, PropertyValue::Float32(val))
    }

    /// Add a dependency edge, recording it for removal (D5). Theme
    /// local names must be fresh (convention: `th_`-prefixed) — a
    /// duplicate local name would shadow another in the eval globals.
    pub fn depend_journaled(
        &self,
        prop: &PropertyPtr,
        dep: &PropertyPtr,
        i: usize,
        local_name: String,
    ) {
        prop.add_depend(Role::Theme, dep, i, local_name.clone());
        let mut journal = self.journal.lock();
        if !journal.iter().any(|e| matches!(e, JournalEntry::Depend { prop: p, dep_prop: d, i: j, local_name: n } if Arc::ptr_eq(p, prop) && Arc::ptr_eq(d, dep) && *j == i && *n == local_name))
        {
            journal.push(JournalEntry::Depend {
                prop: prop.clone(),
                dep_prop: dep.clone(),
                i,
                local_name,
            });
        }
    }

    /// Set a shared token's value on the `/theme` node (stamped
    /// `Role::Theme`, journaled as touched so unload unsets it and the
    /// minimal-palette default shows again).
    pub fn set_shared_token_color(
        &self,
        atom: &mut PropertyAtomicGuard,
        name: &str,
        val: [f32; 4],
    ) -> Result<()> {
        let theme = self.sg_root.lookup_node("/theme").ok_or(Error::NodeNotFound)?;
        let prop = theme.get_property(name).ok_or(Error::PropertyNotFound)?;
        self.set_touched_color(atom, &prop, val)
    }

    /// Single-f32 variant of [`ThemeCtx::set_shared_token_color`].
    pub fn set_shared_token_f32(
        &self,
        atom: &mut PropertyAtomicGuard,
        name: &str,
        val: f32,
    ) -> Result<()> {
        let theme = self.sg_root.lookup_node("/theme").ok_or(Error::NodeNotFound)?;
        let prop = theme.get_property(name).ok_or(Error::PropertyNotFound)?;
        self.set_touched_f32(atom, &prop, val)
    }
}

/// A theme: a name and an apply routine. `minimal` is the registry's
/// identity element — no apply (the minimal look IS the unloaded
/// state), so switching to minimal is just unload.
#[async_trait::async_trait]
pub trait Theme: Send + Sync {
    fn name(&self) -> &'static str;

    async fn apply(&self, ctx: &ThemeCtx) -> Result<()> {
        let _ = ctx;
        Ok(())
    }
}

struct MinimalTheme;

#[async_trait::async_trait]
impl Theme for MinimalTheme {
    fn name(&self) -> &'static str {
        "minimal"
    }
}

/// The default theme when nothing (or something unknown) is persisted.
pub const DEFAULT_THEME: &str = "scifi";

/// Compile-time registry. Dynamic/runtime-loaded themes are a non-goal.
pub fn registry() -> Vec<&'static dyn Theme> {
    vec![&MinimalTheme, &scifi::ScifiTheme]
}

pub fn registry_lookup(name: &str) -> Option<&'static dyn Theme> {
    registry().into_iter().find(|t| t.name() == name)
}

/// Unload the theme recorded in `ctx`. Contract order (design D8):
/// `clear_tasks()` on tracked nodes FIRST (cancel the actors — an
/// in-flight watcher writing `alpha` after its unset would leave a
/// stale value nothing resets) → unset Touched entries → restore List
/// entries → remove Depend edges → `unlink()` per tracked node (drops
/// subtrees and their properties; token children included — edges are
/// off before the nodes holding their targets die, so no eval ever
/// sees a dangling dep).
fn unload(ctx: &ThemeCtx, atom: &mut PropertyAtomicGuard) {
    // 1. Cancel the actors.
    for node in ctx.nodes.lock().iter() {
        node.clear_tasks();
    }

    // 2-4. Replay the journal. Depends come off after value restores
    // so no live watcher can race an edge removal mid-restore.
    let journal: Vec<JournalEntry> = std::mem::take(&mut *ctx.journal.lock());
    let mut depends = vec![];
    for entry in journal {
        match entry {
            JournalEntry::Touched { prop, i } => {
                let _ = prop.unset(atom, Role::Theme, i);
            }
            JournalEntry::Depend { .. } => depends.push(entry),
        }
    }
    for entry in depends {
        if let JournalEntry::Depend { prop, dep_prop, i, local_name } = entry {
            prop.remove_depend(Role::Theme, &dep_prop, i, &local_name);
        }
    }

    // 5. Drop the trees.
    for node in ctx.nodes.lock().iter() {
        node.unlink();
    }
    ctx.nodes.lock().clear();
}

/// Atomic switch flow (design D9): one redraw-guard batch for the whole
/// unload+apply, an explicit trigger afterwards for the structural-only
/// changes (unlinks notify no properties), and the engine-owned
/// `/setting/theme` watcher is never theme-tracked.
pub async fn switch(app: &Arc<App>, next_name: &str) -> Result<String> {
    let next = registry_lookup(next_name)
        .or_else(|| registry_lookup(DEFAULT_THEME))
        .ok_or(Error::ThemeNotFound)?;
    let ctx = ThemeCtx::new(app);
    let atom = &mut app.redraw_trigger.make_guard(gfxtag!("theme switch"));

    // Unload whatever is installed (minimal = nothing to unload).
    if let Some(installed) = app.theme_ctx.lock().take() {
        unload(&installed, atom);
    }

    // Load: the next theme's tokens/nodes/watchers.
    next.apply(&ctx).await?;
    *app.theme_ctx.lock() = Some(ctx);

    Ok(next.name().to_string())
}

/// Read the persisted `theme` setting (unknown → default), apply it,
/// and spawn the engine-owned watcher for live switches. Called from
/// `App::setup` after `schema::make`. Applying before the widget
/// listener tasks exist is safe: pubsub queues are unbounded, so the
/// token-set notifications buffer and drain once `App::start` spawns
/// the poll loops; the first draw pass then evaluates draw-side from
/// the already-settled token values.
pub async fn apply_startup(app: &Arc<App>) {
    let requested =
        app.sg_root.lookup_node("/setting").unwrap().get_property_enum("theme").unwrap();
    // Unknown persisted value falls back to the default (spec).
    let resolved = match switch(app, &requested).await {
        Ok(name) => name,
        Err(e) => {
            error!(target: "theme", "theme switch failed: {e}");
            return;
        }
    };
    info!(target: "theme", "theme '{resolved}' applied");

    // Engine-owned watcher: live switches on /setting/theme changes.
    // Never theme-tracked, never unloaded with a theme.
    let app2 = app.clone();
    let watcher = app.ex.spawn(async move {
        let setting = app2.sg_root.lookup_node("/setting").unwrap();
        let prop = setting.get_property("theme").unwrap();
        let sub: Subscription<_> = prop.subscribe_modify();
        while let Ok(_) = sub.receive().await {
            let name = prop.get_enum(0).unwrap();
            info!(target: "theme", "switching theme to '{name}'");
            if let Err(e) = switch(&app2, &name).await {
                error!(target: "theme", "live theme switch failed: {e}");
            }
        }
    });
    app.tasks.lock().push(watcher);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prop::{
        PropertyAtomicGuard, PropertyPermission, PropertySubType, PropertyType, Role,
    };

    /// Synthetic theme unload verification (task 6.1): touches one
    /// shared token, defines one private token wired to a widget,
    /// overrides a bounded prop and an unbounded list, inserts one
    /// tracked node, and pushes one watcher task — then unloads and
    /// asserts zero residue.
    #[test]
    fn test_theme_ctx_unload_clean() {
        let sg_root = SceneNode::root();
        let theme_node = create_theme_node();
        sg_root.link(theme_node.clone());

        let (redraw, _rx) = RedrawTrigger::new();
        let ctx = ThemeCtx::new_parts(sg_root.clone(), redraw);
        let atom = &mut PropertyAtomicGuard::none();

        // A "widget" node with a themeable color prop (wired to a token)
        let mut widget = SceneNode::new("widget", SceneNodeType::Object);
        let mut color = Property::new(
            "text_color",
            PropertyType::Float32,
            PropertySubType::Color,
            PropertyPermission { read: Role::ALL, write: Role::App | Role::Theme },
        );
        color.set_array_len(4);
        color.allow_exprs();
        widget.add_property(color).unwrap();
        let widget = widget.setup_null();
        sg_root.link(widget.clone());

        // 1. Touch one shared token.
        ctx.set_shared_token_color(atom, "accent_color", [0., 0.94, 1., 1.]).unwrap();
        let accent = theme_node.get_property("accent_color").unwrap();
        assert_eq!(accent.get_f32(0).unwrap(), 0.);
        assert_eq!(accent.get_f32(1).unwrap(), 0.94);

        // 2. Private token child wired to the widget (journaled edge).
        let mut priv_tok = Property::new(
            "glow_color",
            PropertyType::Float32,
            PropertySubType::Color,
            PropertyPermission { read: Role::ALL, write: Role::Theme },
        );
        priv_tok.set_array_len(4);
        priv_tok.set_defaults_f32(vec![1., 0., 1., 1.]).unwrap();
        let tokens = ctx.create_token_child("testtheme", vec![priv_tok]).unwrap();
        let glow = tokens.get_property("glow_color").unwrap();
        let color_prop = widget.get_property("text_color").unwrap();
        for i in 0..4 {
            ctx.depend_journaled(&color_prop, &glow, i, format!("th_glow_{i}"));
        }
        assert_eq!(color_prop.get_depends().len(), 4);

        // 3. Override a bounded prop.
        ctx.set_touched_color(atom, &color_prop, [1., 1., 1., 1.]).unwrap();
        assert_eq!(color_prop.get_f32(0).unwrap(), 1.);

        // 4. Override an unbounded list: defaults are the baseline,
        // vals are the theme's override, clear restores defaults.
        let mut list = Property::new(
            "nick_colors",
            PropertyType::Float32,
            PropertySubType::Color,
            PropertyPermission { read: Role::ALL, write: Role::App | Role::Theme },
        );
        list.set_unbounded();
        let list_holder = {
            let mut n = SceneNode::new("holder", SceneNodeType::Object);
            n.add_property(list).unwrap();
            n.setup_null()
        };
        sg_root.link(list_holder.clone());
        let list_prop = list_holder.get_property("nick_colors").unwrap();
        list_prop.set_default_f32_multi(&[1., 2., 3., 4.]).unwrap();
        assert_eq!(list_prop.get_len(), 4);
        assert_eq!(list_prop.get_f32(0).unwrap(), 1.);
        // Theme overrides the whole list via vals
        list_prop.set_f32_vec(atom, Role::Theme, vec![9.; 8]).unwrap();
        assert_eq!(list_prop.get_len(), 8);
        assert_eq!(list_prop.get_f32(0).unwrap(), 9.);

        // 5. One inserted structural node (tracked).
        let extra = {
            let mut n = SceneNode::new("deco", SceneNodeType::Object);
            let p = Property::new(
                "x",
                PropertyType::Float32,
                PropertySubType::Null,
                PropertyPermission::default(),
            );
            n.add_property(p).unwrap();
            n.setup_null()
        };
        ctx.link_tracked(&widget, extra.clone());

        // 6. One watcher task on a tracked node; it must be cancelled by
        // clear_tasks() during unload (never runs to completion).
        let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let done2 = done.clone();
        let (tx, rx) = smol::channel::bounded::<()>(1);
        let task = smol::Executor::new().spawn(async move {
            let _ = rx.recv().await;
            done2.store(true, std::sync::atomic::Ordering::SeqCst);
        });
        ctx.push_task(&tokens, task);
        // Give the executor a tick to start the task.
        smol::block_on(smol::future::yield_now());

        // Unload (private engine fn — same module).
        unload(&ctx, atom);

        // Token restored to the minimal default.
        assert_eq!(accent.get_f32(0).unwrap(), 0.75);
        assert_eq!(accent.get_f32(1).unwrap(), 0.75);
        assert!(accent.get_raw_value(0).unwrap().is_unset());

        // Bounded override gone: falls to the (unwired) default tier.
        assert!(color_prop.get_raw_value(0).unwrap().is_unset());

        // Journaled dependency edges removed.
        assert!(color_prop.get_depends().is_empty());

        // Unbounded list restored to defaults (vals cleared).
        list_prop.clear_values(atom, Role::Theme).unwrap();
        assert_eq!(list_prop.get_len(), 4);
        assert_eq!(list_prop.get_f32(0).unwrap(), 1.);

        // Tracked nodes unlinked: /theme/testtheme and the deco node are
        // gone from their parents.
        assert!(sg_root.lookup_node("/theme/testtheme").is_none());
        assert!(widget.lookup_node("/deco").is_none());

        // Watcher cancelled: it was waiting on the channel; the task is
        // dropped with clear_tasks, so `done` never flips even when the
        // sender fires afterwards.
        let _ = tx.try_send(());
        smol::block_on(smol::future::yield_now());
        assert!(!done.load(std::sync::atomic::Ordering::SeqCst));
    }

    /// Every shared-token name referenced by schema `wire_*` calls and
    /// by the scifi theme's `set_shared_token_*` writes must exist on
    /// the `/theme` node — a missing one is a runtime
    /// `PropertyNotFound` panic in `schema::make` (found the hard way).
    #[test]
    fn test_shared_token_vocabulary_covers_all_references() {
        let theme = create_theme_node();
        let referenced = [
            // generic
            "text_color",
            "text_dim_color",
            "bg_color",
            "bg_dim_color",
            "bg_overlay_color",
            "accent_color",
            "sep_color",
            // edit
            "edit.text_color",
            "edit.bg_color",
            "edit.hi_bg_color",
            "edit.text_hi_color",
            "edit.cursor_color",
            "edit.placeholder_color",
            "edit.action_fg_color",
            "edit.action_bg_color",
            // menu
            "menu.bg_color",
            "menu.sep_color",
            "menu.role1_color",
            "menu.role2_color",
            // chatview
            "chatview.bg_color",
            "chatview.timestamp_color",
            "chatview.text_color",
            "chatview.hi_bg_color",
            "chatview.action_text_color",
            "chatview.url_text_color",
            // f32
            "font_size",
            "message_spacing",
            "line_height",
        ];
        for name in referenced {
            assert!(theme.get_property(name).is_some(), "missing shared token: {name}");
        }
    }
}
