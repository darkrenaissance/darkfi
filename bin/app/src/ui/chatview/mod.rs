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

//! The chatview widget: a virtualized, scrollable chat log with typed
//! interactive messages. See the `app-chatview` OpenSpec change for the
//! behavioral contract this module implements.
//!
//! Module map (one concern per file):
//!
//! * [`buffer`] — record store: ordering, dedup, geometry. Pure data.
//! * [`codec`] — the kvdb wire format.
//! * [`scroll`] — scroll controller: gestures, animation, clamping,
//!   compensation, save/restore.
//! * `loader` — kvdb → filter → buffer coverage maintenance.
//! * `msg` — message type contract, registry, and per-type nodes.
//!
//! This file is the scene node: properties, view-wide methods and
//! signals, input dispatch, draw assembly, and the animator. It
//! orchestrates; it owns nothing heavy.

use async_lock::Mutex as AsyncMutex;
use async_trait::async_trait;
use darkfi::system::CondVar;
use darkfi_serial::{Decodable, Encodable, SerialDecodable, SerialEncodable};
use kvdb_overlay::{Database as KvDb, Tree};
use parking_lot::Mutex as SyncMutex;
use rand::{rngs::OsRng, Rng};
use smol::Timer;
use std::{
    collections::{HashMap, HashSet},
    io::Cursor,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Weak,
    },
    time::Instant,
};

use miniquad::{KeyCode, KeyMods, MouseButton};

use crate::{
    gfx::{gfxtag, DrawCall, DrawInstruction, Point, Rectangle, RenderApi, Renderer},
    mesh::{Color, MeshBuilder},
    prop::{
        PropertyAtomicGuard, PropertyBool, PropertyColor, PropertyFloat32, PropertyRect,
        PropertyStr, PropertyUint32, Role,
    },
    scene::{MethodCallSub, Pimpl, SceneNodeWeak},
    text,
    util::{clipboard, i18n::I18nBabelFish},
    ExecutorPtr,
};

use super::{DrawUpdate, GestureAction, GestureSet, OnModify, RedrawTrigger, UIObject};

pub mod buffer;
pub mod codec;
pub mod loader;
pub mod msg;
pub mod scroll;

pub use buffer::{MsgBuffer, MsgRecord};
pub use loader::{FilterFn, Loader, Wakeup};
pub use scroll::{Anchor, ScrollController, ScrollState};

macro_rules! t { ($($arg:tt)*) => { trace!(target: "ui::chatview", $($arg)*); } }
macro_rules! i { ($($arg:tt)*) => { info!(target: "ui::chatview", $($arg)*); } }

/// Unix-millisecond message timestamps.
pub type Timestamp = u64;

/// A message's unique identity. The 32 bytes come from darkirc's
/// message hash; derived records (date separators) use the zero id with
/// a synthetic timestamp instead.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, SerialEncodable, SerialDecodable,
)]
pub struct MessageId(pub [u8; 32]);

impl std::fmt::Display for MessageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for b in &self.0 {
            write!(f, "{b:02x}")?
        }
        Ok(())
    }
}

/// Every message type, hardcoded — a fixed enum, no factories. The wire
/// tag IS the discriminant (`#[repr(u8)]`): encode with `as u8`, decode
/// via [`codec::msg_type_from_u8`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum MsgType {
    PrivMsg = 0,
    FileMsg = 1,
    DateMsg = 2,
}

impl MsgType {
    /// Whether records of this type are derived from other records
    /// rather than persisted. Derived records never enter the id index:
    /// their synthetic ids (e.g. the date separators' zero id) are not
    /// unique across records.
    pub fn is_derived(&self) -> bool {
        matches!(self, Self::DateMsg)
    }
}

pub type ChatViewPtr = Arc<ChatView>;

/// The chatview scene node. View-wide concerns only: channel
/// switching, filtering, selection state, scrolling surface, and draw
/// assembly of the visible window. Message lifecycle operations live on
/// the per-type sub-nodes.
pub struct ChatView {
    node: SceneNodeWeak,
    tasks: SyncMutex<Vec<smol::Task<()>>>,
    renderer: Renderer,
    redraw: RedrawTrigger,
    ex: ExecutorPtr,

    /// The chat kvdb; channel trees are opened from it on demand.
    kv_db: KvDb,
    /// The bound channel and its tree.
    channel: SyncMutex<Option<(String, Tree)>>,

    buffer: Arc<AsyncMutex<MsgBuffer>>,
    controller: SyncMutex<ScrollController>,
    /// Wakes the animator task when motion begins.
    motion_cv: Arc<CondVar>,

    /// The single background loading pipeline.
    loader: Arc<Loader>,
    /// The per-type message nodes, created right after construction
    /// (they need a weak self reference, which only exists then).
    types: std::sync::OnceLock<Arc<msg::TypeNodes>>,
    /// Per-channel scroll anchors, saved on channel exit.
    channel_state: SyncMutex<HashMap<String, Anchor>>,
    /// Anchor awaiting enough coverage to resolve on re-entry.
    pending_restore: SyncMutex<Option<Anchor>>,

    dc_key: u64,

    rect: PropertyRect,
    z_index: PropertyUint32,
    priority: PropertyUint32,
    is_at_bottom: PropertyBool,
    hi_bg_color: PropertyColor,
    wheel_page_frac: PropertyFloat32,
    channel_prop: crate::prop::PropertyStr,

    /// Selected messages by composite key; view-wide, type-agnostic.
    /// The composite (not the bare id) because derived records share
    /// synthetic ids.
    selected: SyncMutex<HashSet<(Timestamp, MessageId)>>,
    /// Last reported selection presence, for `select_changed` edges.
    select_active: AtomicBool,
    /// Mouse position for wheel hit-testing.
    mouse_pos: SyncMutex<Point>,
    mouse_btn_held: AtomicBool,
    /// In-progress mouse selection gesture.
    select_drag: SyncMutex<Option<SelectDrag>>,
    /// What the fired long-press chose for this touch.
    lp_mode: SyncMutex<LongPressMode>,
    /// Active copy-link overlay (None when hidden).
    link_toast: SyncMutex<Option<LinkToast>>,
    /// Re-arm counter so a stale dismiss task won't clear a newer toast.
    toast_version: std::sync::atomic::AtomicU32,
    /// Window scale (for building the toast text layout).
    window_scale: PropertyFloat32,
    /// Geometry the last draw pass saw, for reflow detection.
    last_width: SyncMutex<f32>,
    last_scale: SyncMutex<f32>,

    /// Weak self-reference so handlers can spawn detached tasks.
    me: Weak<Self>,
}

/// Mouse must move more than this many pixels while held to count as a
/// drag (which only selects) rather than a click (which toggles).
const SELECT_DRAG_THRESHOLD: f32 = 2.;

/// Records of soft margin above/below the visible range kept
/// materialized (the virtualization window's give).
const SOFT_WINDOW_MARGIN: usize = 40;
/// Out-of-window instances kept per type, most recently used first.
const INSTANCE_BUDGET: usize = 150;

/// Tracks an in-progress mouse selection gesture so we can distinguish a
/// stationary click (toggles the line) from a drag (only ever selects).
struct SelectDrag {
    down_y: f32,
    was_selected: bool,
    dragged: bool,
}

/// What the fired long-press chose for this touch.
#[derive(Clone, Copy, PartialEq)]
enum LongPressMode {
    /// No long-press fired
    None,
    /// Line selection started; drags extend it, no scroll inertia
    Select,
    /// URL copied with a toast; drags scroll, inertia allowed
    UrlToast,
}

/// Transient "Copied link" overlay state. Rendered as an
/// `DrawInstruction::Overlay` in its own draw call so it floats above
/// the link and escapes the message clip rect.
struct LinkToast {
    text_layout: text::TextLayout,
    anchor: Point,
    offset: f32,
    fg_color: Color,
    bg_color: Color,
    font_size: f32,
    padding: f32,
}

impl LinkToast {
    /// The overlay draw-instructions: a filled background box with an
    /// fg-colored outline and the label centered inside, drawn above
    /// the anchor point (shifted up by `offset`).
    fn build_instrs(&self, renderer: &Renderer) -> Vec<DrawInstruction> {
        let text_w = self.text_layout.width();
        let text_h = self.text_layout.height();
        let pad = self.padding;
        let box_w = text_w + 2. * pad;
        let box_h = self.font_size + 2. * pad;

        // Box bottom sits `offset` above the anchor (negative y is up).
        let pos = Point::new(self.anchor.x, self.anchor.y - self.offset);

        let mut instrs = vec![DrawInstruction::Move(pos)];

        // Box: top at -box_h, bottom at 0.
        instrs.push(DrawInstruction::Move(Point::new(0., -box_h)));
        let bg_rect = Rectangle::new(0., 0., box_w, box_h);
        let mut mesh = MeshBuilder::new(gfxtag!("chatview_urltoast_bg"));
        mesh.draw_filled_box(&bg_rect, self.bg_color);
        mesh.draw_outline(&bg_rect, self.fg_color, 1.);
        instrs.push(DrawInstruction::Draw(mesh.alloc(renderer).draw_untextured()));

        // Label: inset horizontally by padding, centered vertically.
        let text_y = (box_h - text_h) / 2.;
        instrs.push(DrawInstruction::Move(Point::new(pad, text_y)));
        let mut txt_instrs =
            text::render_layout(&self.text_layout, renderer, gfxtag!("chatview_urltoast_txt"));
        instrs.append(&mut txt_instrs);

        instrs
    }
}

impl ChatView {
    pub async fn new(
        node: SceneNodeWeak,
        kv_db: KvDb,
        window_scale: PropertyFloat32,
        i18n_fish: I18nBabelFish,
        renderer: Renderer,
        redraw: RedrawTrigger,
        ex: ExecutorPtr,
    ) -> Pimpl {
        let node_ref = &node.upgrade().unwrap();
        let rect = PropertyRect::wrap(node_ref, Role::Internal, "rect").unwrap();
        let z_index = PropertyUint32::wrap(node_ref, Role::Internal, "z_index", 0).unwrap();
        let priority = PropertyUint32::wrap(node_ref, Role::Internal, "priority", 0).unwrap();
        let is_at_bottom = PropertyBool::wrap(node_ref, Role::Internal, "is_at_bottom", 0).unwrap();

        let buffer = Arc::new(AsyncMutex::new(MsgBuffer::new()));
        let loader = Loader::new(buffer.clone(), redraw.clone());

        let self_ = Arc::new_cyclic(|me| Self {
            node: node.clone(),
            tasks: SyncMutex::new(vec![]),
            renderer: renderer.clone(),
            redraw: redraw.clone(),
            ex,

            kv_db,
            channel: SyncMutex::new(None),

            buffer,
            controller: SyncMutex::new(ScrollController::new()),
            motion_cv: Arc::new(CondVar::new()),

            loader,
            types: std::sync::OnceLock::new(),
            channel_state: SyncMutex::new(HashMap::new()),
            pending_restore: SyncMutex::new(None),

            dc_key: OsRng.gen(),

            rect,
            z_index,
            priority,
            is_at_bottom,
            hi_bg_color: PropertyColor::wrap(node_ref, Role::Internal, "hi_bg_color")
                .expect("chatview hi_bg_color"),
            wheel_page_frac: PropertyFloat32::wrap(
                node_ref,
                Role::Internal,
                "wheel_page_frac",
                0,
            )
            .expect("chatview wheel_page_frac"),
            channel_prop: crate::prop::PropertyStr::wrap(node_ref, Role::Internal, "channel", 0)
                .expect("chatview channel"),

            selected: SyncMutex::new(HashSet::new()),
            select_active: AtomicBool::new(false),
            mouse_pos: SyncMutex::new(Point::from([0., 0.])),
            mouse_btn_held: AtomicBool::new(false),
            select_drag: SyncMutex::new(None),
            lp_mode: SyncMutex::new(LongPressMode::None),
            link_toast: SyncMutex::new(None),
            toast_version: std::sync::atomic::AtomicU32::new(0),
            window_scale: window_scale.clone(),
            last_width: SyncMutex::new(0.),
            last_scale: SyncMutex::new(window_scale.get()),

            me: me.clone(),
        });

        // The type nodes are children of this node; they need a weak
        // self, so they can only be created once `self_` exists.
        let shared = msg::SharedProps::wrap(&node.upgrade().unwrap(), window_scale);
        let types = msg::TypeNodes::new(
            &node.upgrade().unwrap(),
            shared,
            i18n_fish,
            self_.loader.clone(),
            self_.buffer.clone(),
            Arc::downgrade(&self_),
        )
        .await;
        let types = Arc::new(types);
        let loader_measure = types.clone();
        self_.loader.set_measure(Arc::new(move |rec| loader_measure.measure(rec)));
        self_.loader.set_derive(Arc::new(|rec: &MsgRecord| {
            let (_, text, _) = codec::decode_privmsg_payload(&rec.payload, rec.ts, &rec.id);
            msg::filemsg::derive_filemsg(rec, &text)
        }));
        if self_.types.set(types).is_err() {
            panic!("type nodes set twice")
        }

        self_.sync_is_at_bottom();
        Pimpl::ChatView(self_)
    }

    /// The type-node registry.
    fn types(&self) -> &Arc<msg::TypeNodes> {
        self.types.get().expect("type nodes initialized")
    }

    /// Derive a file message from a just-inserted privmsg whose text
    /// carries a fud URL: buffer insert + measure + signal.
    pub async fn derive_filemsg(&self, privmsg: &MsgRecord, nick: &str, text: &str) {
        let Some(file_rec) = msg::filemsg::derive_filemsg(privmsg, text) else { return };
        let url =
            msg::filemsg::decode_filemsg_payload(&file_rec.payload, file_rec.ts, &file_rec.id);
        let height = self.types().filemsg.measure(&file_rec);

        let mut buffer = self.buffer.lock().await;
        if !buffer.insert(MsgRecord { height, ..file_rec }) {
            return
        }
        drop(buffer);

        if let Some(node) = self.types().filemsg.node().upgrade() {
            let mut data = vec![];
            darkfi_serial::Encodable::encode(&url, &mut data).unwrap();
            let _ = node.trigger("fileurl_detected", data).await;
        }
        t!("derived filemsg for {nick}: {url}");
    }

    /// The channel tree name for a channel: `{channel}__chat_tree_v2`.
    /// Versioned so the clean-break format never reads old trees.
    pub fn tree_name(channel: &str) -> String {
        format!("{channel}__chat_tree_v2")
    }

    /// Update the `is_at_bottom` property when the position crosses the
    /// bottom. Externally visible state for the down-arrow layer.
    fn sync_is_at_bottom(&self) {
        let at_bottom = self.controller.lock().is_at_bottom();
        if self.is_at_bottom.get() == at_bottom {
            return
        }
        let atom = &mut self.redraw.make_guard(gfxtag!("chatview is_at_bottom"));
        self.is_at_bottom.set(atom, at_bottom);
    }

    /// Wake the animator: motion may have begun.
    fn notify_motion(&self) {
        self.motion_cv.notify();
    }

    /// Convert a screen-space y to content px from the bottom.
    async fn content_y(&self, screen_y: f32) -> f32 {
        let rect = self.rect.get();
        let scroll = self.controller.lock().scroll();
        rect.h - (screen_y - rect.y) + scroll
    }

    /// Mark the line under screen y as selected.
    async fn select_line(&self, screen_y: f32) {
        let y = self.content_y(screen_y).await;
        let mut buffer = self.buffer.lock().await;
        let Some(rec) = buffer.record_at_y(y) else { return };
        let key = (rec.ts, rec.id);
        drop(buffer);

        let mut selected = self.selected.lock();
        let had = !selected.is_empty();
        selected.insert(key);
        let has = !selected.is_empty();
        drop(selected);

        self.redraw.trigger();
        self.notify_select_changed(has, had).await;
    }

    /// Mark the line under screen y as deselected.
    async fn deselect_line(&self, screen_y: f32) {
        let y = self.content_y(screen_y).await;
        let mut buffer = self.buffer.lock().await;
        let Some(rec) = buffer.record_at_y(y) else { return };
        let key = (rec.ts, rec.id);
        drop(buffer);

        let mut selected = self.selected.lock();
        let had = !selected.is_empty();
        selected.remove(&key);
        let has = !selected.is_empty();
        drop(selected);

        self.redraw.trigger();
        self.notify_select_changed(has, had).await;
    }

    /// Query whether the line under screen y is currently selected.
    async fn is_line_selected(&self, screen_y: f32) -> bool {
        let y = self.content_y(screen_y).await;
        let buffer = self.buffer.lock().await;
        let Some(rec) = buffer.record_at_y(y) else { return false };
        self.selected.lock().contains(&(rec.ts, rec.id))
    }

    /// Emit `select_changed(true/false)` whenever the presence of any
    /// selected line transitions.
    async fn notify_select_changed(&self, has: bool, had: bool) {
        if has == had {
            return
        }
        self.select_active.store(has, Ordering::Relaxed);
        let Some(node_ref) = self.node.upgrade() else { return };
        let mut data = vec![];
        darkfi_serial::Encodable::encode(&has, &mut data).unwrap();
        let _ = node_ref.trigger("select_changed", data).await;
    }

    /// Copy the selected messages' text to the clipboard in display
    /// order, then unselect everything.
    async fn handle_copy_select(&self) {
        {
            let buffer = self.buffer.lock().await;
            let selected = self.selected.lock();
            let text = copy_selected(&buffer, &selected, self.types());
            drop(selected);
            drop(buffer);
            if !text.is_empty() {
                t!("copy_select() [text={text}]");
                clipboard::set(&text);
            }
        }
        self.handle_unselect().await;
    }

    /// Deselect every selected message and redraw.
    async fn handle_unselect(&self) {
        let had = {
            let mut selected = self.selected.lock();
            let had = !selected.is_empty();
            selected.clear();
            had
        };
        self.redraw.trigger();
        self.notify_select_changed(false, had).await;
    }

    /// Resolve the content hit at a screen position, if any.
    async fn content_hit(&self, screen_pos: Point) -> Option<(MessageId, msg::Hit)> {
        let content_y = self.content_y(screen_pos.y).await;
        let rect = self.rect.get();
        let buffer = self.buffer.lock().await;
        let rec = buffer.record_at_y(content_y)?;
        let id = rec.id;
        let top = buffer.pos_of_key(&(rec.ts, rec.id))?;
        let local = Point::new(screen_pos.x - rect.x, top - content_y);
        let hit = self.types().hit_test(rec, local)?;
        Some((id, hit))
    }

    /// Hit-dispatch through the type registry: opens URLs, emits the
    /// type's interaction signals. Returns true when consumed.
    async fn dispatch_content_hit(&self, screen_pos: Point) -> bool {
        let Some((id, hit)) = self.content_hit(screen_pos).await else { return false };

        match hit {
            msg::Hit::Url(url) => {
                t!("url clicked");
                if let Some(node) = self.types().privmsg.node().upgrade() {
                    let mut data = vec![];
                    darkfi_serial::Encodable::encode(&id, &mut data).unwrap();
                    darkfi_serial::Encodable::encode(&url, &mut data).unwrap();
                    let _ = node.trigger("url_clicked", data).await;
                }
                Self::open_url(&url);
                true
            }
            msg::Hit::Nick(nick) => {
                t!("nick clicked");
                if let Some(node) = self.types().privmsg.node().upgrade() {
                    let mut data = vec![];
                    darkfi_serial::Encodable::encode(&id, &mut data).unwrap();
                    darkfi_serial::Encodable::encode(&nick, &mut data).unwrap();
                    let _ = node.trigger("nick_clicked", data).await;
                }
                true
            }
            msg::Hit::File(url) => {
                t!("file activated");
                self.types().filemsg.request_download(&id, &url).await;
                true
            }
            msg::Hit::Expand => {
                // Toggle a capped message's expansion; the height change
                // flows into geometry with scroll compensation.
                let rec = {
                    let buffer = self.buffer.lock().await;
                    buffer.record(&id).cloned()
                };
                let Some(rec) = rec else { return false };
                let new_height = self.types().privmsg.toggle_expand(&rec);
                let mut buffer = self.buffer.lock().await;
                if let Some(delta) = buffer.set_height_key(&(rec.ts, rec.id), new_height) {
                    drop(buffer);
                    let mut ctl = self.controller.lock();
                    let scroll = ctl.scroll();
                    ctl.compensate(delta, self.top_for(id, scroll).await);
                }
                self.redraw.trigger();
                true
            }
        }
    }

    /// Whether the message with this id sits entirely below the
    /// viewport bottom (compensation input).
    async fn top_for(&self, id: MessageId, scroll: f32) -> bool {
        let buffer = self.buffer.lock().await;
        match buffer.pos_of(&id) {
            Some(top) => top <= scroll,
            None => false,
        }
    }

    fn open_url(url: &str) {
        if url.chars().any(|c| c.is_control()) {
            error!(target: "ui::chatview", "refusing to open URL with control characters");
            return
        }
        #[cfg(target_os = "android")]
        crate::android::open_url(url);

        #[cfg(not(target_os = "android"))]
        let _ = open::that(url);
    }

    /// Copy `url` to the clipboard and show the "Copied link" overlay
    /// above `anchor` (chatview-local coords) for `url_copy_duration`
    /// seconds. Re-arms on repeat.
    async fn show_toast(&self, url: &str, anchor: Point) {
        clipboard::set(url);

        let privmsg = self.types().privmsg.clone();
        let label = privmsg.url_copy_text();
        let fg = privmsg.url_copy_fg_color();
        let bg = privmsg.url_copy_bg_color();
        let font_size = privmsg.url_copy_font_size();
        let pad = privmsg.url_copy_padding();
        let offset = privmsg.url_copy_offset();
        let window_scale = self.window_scale.get();

        // A real line ratio (the old code passed 0., which collapses the
        // line box so height() reports 0 and the label centers wrong).
        let text_layout = text::make_layout(&label, fg, font_size, 1.2, window_scale, None, &[]);
        let toast = LinkToast {
            text_layout,
            anchor,
            offset,
            fg_color: fg,
            bg_color: bg,
            font_size,
            padding: pad,
        };
        *self.link_toast.lock() = Some(toast);
        self.redraw.trigger();

        // (Re)arm the dismiss task. A stale task won't clear a newer
        // toast (version check).
        let version = self.toast_version.fetch_add(1, Ordering::SeqCst) + 1;
        let duration = self.types().privmsg.url_copy_duration();
        let me = self.me.clone();
        let ex = self.ex.clone();
        ex.spawn(async move {
            darkfi::system::msleep((duration * 1000.) as u64).await;
            let Some(self_) = me.upgrade() else { return };
            if self_.toast_version.load(Ordering::SeqCst) == version {
                *self_.link_toast.lock() = None;
                self_.redraw.trigger();
            }
        })
        .detach();
    }

    /// Tap resolved by the recognizer: content activation, or line
    /// toggling when a selection is active.
    async fn handle_tap(&self, pos: Point) {
        // URLs and nicks consume the tap; otherwise a selection toggles.
        if self.dispatch_content_hit(pos).await {
            return
        }
        if self.select_active.load(Ordering::Relaxed) {
            if self.is_line_selected(pos.y).await {
                self.deselect_line(pos.y).await;
            } else {
                self.select_line(pos.y).await;
            }
        }
    }

    /// Long-press resolved by the recognizer: a URL under the finger is
    /// copied with a toast (drags keep scrolling); otherwise line
    /// selection starts and drags extend it instead of scrolling.
    async fn handle_long_press(&self, pos: Point) {
        if let Some((_, msg::Hit::Url(url))) = self.content_hit(pos).await {
            *self.lp_mode.lock() = LongPressMode::UrlToast;
            self.show_toast(&url, pos - self.rect.get().pos()).await;
            return
        }

        *self.lp_mode.lock() = LongPressMode::Select;
        self.select_line(pos.y).await;
    }

    /// Bind to a channel: save the outgoing channel's scroll anchor,
    /// release its in-memory buffer, and open the target channel's
    /// tree. The loader refills in the background; the scroll restore
    /// resolves when coverage reaches the saved anchor.
    pub async fn handle_set_channel(&self, channel: String) {
        t!("set_channel({channel})");
        let view_h = self.rect.get().h;

        // Save where the user left the outgoing channel.
        if let Some((old_name, _)) = self.channel.lock().as_ref() {
            let buffer = self.buffer.lock().await;
            let oldest_visible = {
                let scroll = self.controller.lock().scroll();
                let end = buffer.visible_range(scroll, view_h).end;
                end.checked_sub(1).and_then(|idx| buffer.record_at(idx)).map(|rec| rec.id)
            };
            let anchor = self
                .controller
                .lock()
                .anchor(view_h, oldest_visible.as_ref(), |id| buffer.pos_of(id));
            self.channel_state.lock().insert(old_name.clone(), anchor);
        }

        self.types().release_all();
        self.buffer.lock().await.clear();

        let tree_name = Self::tree_name(&channel);
        let tree = self
            .kv_db
            .open_tree_default(&tree_name)
            .unwrap_or_else(|e| panic!("cannot open chat tree {tree_name}: {e}"));
        *self.channel.lock() = Some((channel.clone(), tree.clone()));
        {
            let atom = &mut self.redraw.make_guard(gfxtag!("chatview channel"));
            self.channel_prop.set(atom, &channel);
        }

        // Restore the incoming channel's anchor (bottom by default);
        // it resolves once the loader's coverage reaches it.
        let restore = self.channel_state.lock().get(&channel).cloned();
        *self.pending_restore.lock() = restore;
        self.controller.lock().scroll_to_bottom();
        self.sync_is_at_bottom();

        self.loader.bind(channel, tree);
        self.redraw.trigger();
    }

    /// Receive a message from the relay: messages for the bound
    /// channel insert live; anything else is persisted to its own
    /// channel tree for later (the buffer is not touched — unread
    /// indication is the relayer's concern).
    pub async fn handle_receive(
        &self,
        channel: &str,
        ts: Timestamp,
        id: MessageId,
        nick: String,
        text: String,
    ) {
        let active = self.channel.lock().as_ref().map(|(name, _)| name.clone());
        match active {
            Some(active) if active == channel => {
                self.types().privmsg.insert_line(ts, id, nick, text).await;
            }
            _ => {
                // Side-channel store: dedup by composite key, codec write.
                let tree_name = Self::tree_name(channel);
                let Ok(tree) = self.kv_db.open_tree_default(&tree_name) else {
                    error!(target: "ui::chatview", "cannot open side-channel tree {tree_name}");
                    return
                };
                let payload = codec::encode_privmsg_payload(&nick, &text, true);
                let key = codec::encode_key(ts, &id);
                match tree.contains_key(&key) {
                    Ok(false) => {
                        let val = codec::encode_value(MsgType::PrivMsg, &payload);
                        tree.insert(&key, &val).expect("cannot persist chat entry");
                        t!("side-channel store {channel} ts={ts}");
                    }
                    _ => {}
                }
            }
        }
    }

    /// Replace the runtime message filter (a callback cannot travel the
    /// scene method bus; this is a native API).
    pub fn set_filter(&self, f: Box<dyn Fn(&MsgRecord) -> bool + Send>) {
        self.loader.set_filter(f);
    }

    /// The reflow protocol for width, scale, and styling changes:
    /// anchor snapshot, drop rendered state, re-wrap visible-first,
    /// one Fenwick rebuild, anchor restore. Height-only rect changes
    /// do NOT come here — they just re-clamp via `set_content`.
    pub async fn reflow(&self) {
        t!("reflow begin");
        let view_h = self.rect.get().h;

        // 1. Snapshot the view position before invalidating anything.
        let anchor = {
            let buffer = self.buffer.lock().await;
            let scroll = self.controller.lock().scroll();
            let oldest_visible = {
                let end = buffer.visible_range(scroll, view_h).end;
                end.checked_sub(1).and_then(|idx| buffer.record_at(idx)).map(|rec| rec.id)
            };
            let mut pos_of = |id: &MessageId| buffer.pos_of(id);
            self.controller.lock().anchor(view_h, oldest_visible.as_ref(), &mut pos_of)
        };

        // 2. Drop all rendered state (layouts, meshes, hit rects).
        self.types().regen_all();

        // 3. Re-wrap every loaded record, visible window first so the
        //    restored frame shows correct content immediately, then a
        //    single Fenwick rebuild.
        let types = self.types().clone();
        {
            let mut buffer = self.buffer.lock().await;
            let scroll = self.controller.lock().scroll();
            let visible = buffer.visible_range(scroll, view_h);
            let len = buffer.len();
            let mut order: Vec<usize> = vec![];
            for idx in visible {
                order.push(idx);
            }
            for idx in 0..len {
                if !order.contains(&idx) {
                    order.push(idx);
                }
            }
            // Collect records first (measure locks the type nodes, the
            // buffer guard must not alias through the closure).
            let mut recs = vec![];
            for idx in order {
                if let Some(rec) = buffer.record_at(idx) {
                    recs.push(rec.clone());
                }
            }
            let heights: Vec<(Timestamp, MessageId, f32)> = {
                let mut measured = vec![];
                for rec in &recs {
                    measured.push((rec.ts, rec.id, types.measure(rec)));
                }
                measured
            };
            for (ts, id, h) in heights {
                if let Some(rec) = buffer.record_mut(&id) {
                    if rec.ts == ts {
                        rec.height = h;
                    }
                }
            }
            buffer.rebuild_fenwick();
            t!("reflow remeasured {} records, one fenwick rebuild", recs.len());

            // 4. Resolve the anchor against the new geometry.
            let total = buffer.total_height();
            let mut pos_of = |id: &MessageId| buffer.pos_of(id);
            {
                let mut ctl = self.controller.lock();
                ctl.set_content(total, view_h);
                ctl.restore(&anchor, view_h, &mut pos_of);
            }
        }

        self.sync_is_at_bottom();
        self.redraw.trigger();
        t!("reflow done");
    }

    /// Try resolving a pending scroll restore against the loaded
    /// coverage. Called from the draw path after geometry updates.
    fn try_pending_restore(&self, buffer: &MsgBuffer, view_h: f32) -> bool {
        let Some(anchor) = self.pending_restore.lock().clone() else { return false };
        let Some(msg) = anchor.msg else {
            // Bottom shortcut: nothing to wait for.
            *self.pending_restore.lock() = None;
            return false
        };

        let Some(pos) = buffer.pos_of(&msg) else { return false };
        // Wait until the loaded region actually spans the anchor.
        let target = pos + anchor.dy - view_h;
        if target > (buffer.total_height() - view_h).max(0.) {
            return false
        }

        let mut ctl = self.controller.lock();
        ctl.restore(&anchor, view_h, |id| buffer.pos_of(id));
        drop(ctl);
        *self.pending_restore.lock() = None;
        self.sync_is_at_bottom();
        true
    }

    /// The `(ts, id)` pairs of loaded records in display order.
    async fn handle_get_line_ids(&self) -> Vec<(Timestamp, MessageId)> {
        let buffer = self.buffer.lock().await;
        let mut lines = vec![];
        for rec in buffer.iter_display_order() {
            lines.push((rec.ts, rec.id));
        }
        lines
    }

    /// Delete a loaded message by id: buffer + channel tree. A testing
    /// affordance, not a user-facing feature.
    async fn handle_delete_line(&self, id: MessageId) -> bool {
        let mut buffer = self.buffer.lock().await;
        let Some(ts) = buffer.record(&id).map(|rec| rec.ts) else { return false };
        if !buffer.remove(&id) {
            return false
        }
        drop(buffer);

        let channel = self.channel.lock();
        if let Some((_, tree)) = channel.as_ref() {
            let key = codec::encode_key(ts, &id);
            tree.remove(&key).unwrap();
        }

        self.redraw.trigger();
        true
    }

    /// The down-arrow: teleport to the live bottom, cancel all motion.
    async fn handle_scroll_to_bottom(&self) {
        self.controller.lock().scroll_to_bottom();
        self.sync_is_at_bottom();
        self.redraw.trigger();
    }

    // Method-call handlers, one per scene method.

    async fn process_set_channel_method(me: &Weak<Self>, sub: &MethodCallSub) -> bool {
        let Ok(method_call) = sub.receive().await else {
            t!("set_channel relayer closed");
            return false
        };
        assert!(method_call.send_res.is_none());

        let Ok(channel) = String::decode(&mut Cursor::new(&method_call.data)) else {
            t!("set_channel() method invalid arg data");
            return true
        };

        let Some(self_) = me.upgrade() else {
            panic!("self destroyed before set_channel task stopped")
        };
        self_.handle_set_channel(channel).await;
        true
    }

    async fn process_get_line_ids_method(me: &Weak<Self>, sub: &MethodCallSub) -> bool {
        let Ok(method_call) = sub.receive().await else {
            t!("get_line_ids relayer closed");
            return false
        };

        let Some(self_) = me.upgrade() else {
            panic!("self destroyed before get_line_ids task stopped")
        };
        let lines = self_.handle_get_line_ids().await;
        let mut data = vec![];
        for (ts, id) in lines {
            ts.encode(&mut data).unwrap();
            id.encode(&mut data).unwrap();
        }
        if let Some(send_res) = method_call.send_res {
            let _ = send_res.send(data).await;
        }
        true
    }

    async fn process_delete_line_method(me: &Weak<Self>, sub: &MethodCallSub) -> bool {
        let Ok(method_call) = sub.receive().await else {
            t!("delete_line relayer closed");
            return false
        };
        assert!(method_call.send_res.is_none());

        let Ok(id) = MessageId::decode(&mut Cursor::new(&method_call.data)) else {
            t!("delete_line() method invalid arg data");
            return true
        };

        let Some(self_) = me.upgrade() else {
            panic!("self destroyed before delete_line task stopped")
        };
        self_.handle_delete_line(id).await;
        true
    }

    async fn process_copy_select_method(me: &Weak<Self>, sub: &MethodCallSub) -> bool {
        let Ok(method_call) = sub.receive().await else {
            t!("copy_select relayer closed");
            return false
        };
        assert!(method_call.send_res.is_none());
        assert!(method_call.data.is_empty());

        let Some(self_) = me.upgrade() else {
            panic!("self destroyed before copy_select task stopped")
        };
        self_.handle_copy_select().await;
        true
    }

    async fn process_unselect_method(me: &Weak<Self>, sub: &MethodCallSub) -> bool {
        let Ok(method_call) = sub.receive().await else {
            t!("unselect relayer closed");
            return false
        };
        assert!(method_call.send_res.is_none());
        assert!(method_call.data.is_empty());

        let Some(self_) = me.upgrade() else {
            panic!("self destroyed before unselect task stopped")
        };
        self_.handle_unselect().await;
        true
    }

    async fn process_scroll_to_bottom_method(me: &Weak<Self>, sub: &MethodCallSub) -> bool {
        let Ok(method_call) = sub.receive().await else {
            t!("scroll_to_bottom relayer closed");
            return false
        };
        assert!(method_call.send_res.is_none());
        assert!(method_call.data.is_empty());

        let Some(self_) = me.upgrade() else {
            panic!("self destroyed before scroll_to_bottom task stopped")
        };
        self_.handle_scroll_to_bottom().await;
        true
    }

    async fn process_insert_line_method(
        me: &Weak<Self>,
        sub: &MethodCallSub,
        confirmed: bool,
    ) -> bool {
        let Ok(method_call) = sub.receive().await else {
            t!("insert_line relayer closed");
            return false
        };
        assert!(method_call.send_res.is_none());

        let Some((ts, id, nick, text)) = msg::privmsg::decode_insert_data(&method_call.data) else {
            t!("insert_line() method invalid arg data");
            return true
        };

        let Some(self_) = me.upgrade() else {
            panic!("self destroyed before insert_line task stopped")
        };
        let privmsg = self_.types().privmsg.clone();
        if confirmed {
            privmsg.insert_line(ts, id, nick, text).await;
        } else {
            privmsg.insert_unconf_line(ts, id, nick, text).await;
        }
        true
    }

    async fn process_receive_method(me: &Weak<Self>, sub: &MethodCallSub) -> bool {
        let Ok(method_call) = sub.receive().await else {
            t!("receive relayer closed");
            return false
        };
        assert!(method_call.send_res.is_none());

        let mut cur = Cursor::new(&method_call.data);
        let (channel, ts, id, nick, text) = match (
            String::decode(&mut cur),
            Timestamp::decode(&mut cur),
            MessageId::decode(&mut cur),
            String::decode(&mut cur),
            String::decode(&mut cur),
        ) {
            (Ok(channel), Ok(ts), Ok(id), Ok(nick), Ok(text)) => (channel, ts, id, nick, text),
            _ => {
                t!("receive() method invalid arg data");
                return true
            }
        };

        let Some(self_) = me.upgrade() else {
            panic!("self destroyed before receive task stopped")
        };
        self_.handle_receive(&channel, ts, id, nick, text).await;
        true
    }

    async fn process_set_file_status_method(me: &Weak<Self>, sub: &MethodCallSub) -> bool {
        let Ok(method_call) = sub.receive().await else {
            t!("set_file_status relayer closed");
            return false
        };
        assert!(method_call.send_res.is_none());

        let mut cur = Cursor::new(&method_call.data);
        let (url, status) =
            match (url::Url::decode(&mut cur), msg::filemsg::FileMsgStatus::decode(&mut cur)) {
                (Ok(url), Ok(status)) => (url, status),
                _ => {
                    t!("set_file_status() method invalid arg data");
                    return true
                }
            };

        let Some(self_) = me.upgrade() else {
            panic!("self destroyed before set_file_status task stopped")
        };
        self_.types().filemsg.set_file_status(&url, status).await;
        true
    }

    async fn process_confirm_method(me: &Weak<Self>, sub: &MethodCallSub) -> bool {
        let Ok(method_call) = sub.receive().await else {
            t!("confirm relayer closed");
            return false
        };
        assert!(method_call.send_res.is_none());

        let Ok(id) = MessageId::decode(&mut Cursor::new(&method_call.data)) else {
            t!("confirm() method invalid arg data");
            return true
        };

        let Some(self_) = me.upgrade() else {
            panic!("self destroyed before confirm task stopped")
        };
        self_.types().privmsg.confirm(id).await;
        true
    }

    /// The animator: advances Glide/Anim on a deadline cadence, driving
    /// redraws (and `is_at_bottom`) while any motion is in flight.
    async fn run_animator(me: &Weak<Self>, cv: Arc<CondVar>) {
        loop {
            cv.wait().await;
            let Some(self_) = me.upgrade() else { break };

            loop {
                let deadline = {
                    let ctl = self_.controller.lock();
                    let now = Instant::now();
                    let Some(deadline) = ctl.next_deadline(now) else { break };
                    deadline
                };
                let now = Instant::now();
                if deadline > now {
                    Timer::after(deadline - now).await;
                }

                let advanced = self_.controller.lock().tick(Instant::now()).is_some();
                if advanced {
                    self_.sync_is_at_bottom();
                    self_.redraw.trigger();
                }
            }

            cv.reset();
        }
    }
}

#[async_trait]
impl UIObject for ChatView {
    fn priority(&self) -> u32 {
        self.priority.get()
    }

    async fn start(self: Arc<Self>, ex: ExecutorPtr) {
        let node_ref = self.node.upgrade().unwrap();

        let mut tasks = vec![];

        let sub = node_ref.subscribe_method_call("set_channel").unwrap();
        let me2 = self.me.clone();
        tasks.push(
            ex.spawn(async move { while Self::process_set_channel_method(&me2, &sub).await {} }),
        );

        let sub = node_ref.subscribe_method_call("get_line_ids").unwrap();
        let me2 = self.me.clone();
        tasks.push(
            ex.spawn(async move { while Self::process_get_line_ids_method(&me2, &sub).await {} }),
        );

        let sub = node_ref.subscribe_method_call("delete_line").unwrap();
        let me2 = self.me.clone();
        tasks.push(
            ex.spawn(async move { while Self::process_delete_line_method(&me2, &sub).await {} }),
        );

        let sub = node_ref.subscribe_method_call("copy_select").unwrap();
        let me2 = self.me.clone();
        tasks.push(
            ex.spawn(async move { while Self::process_copy_select_method(&me2, &sub).await {} }),
        );

        let sub = node_ref.subscribe_method_call("unselect").unwrap();
        let me2 = self.me.clone();
        tasks.push(
            ex.spawn(async move { while Self::process_unselect_method(&me2, &sub).await {} }),
        );

        let sub = node_ref.subscribe_method_call("scroll_to_bottom").unwrap();
        let me2 = self.me.clone();
        tasks.push(ex.spawn(async move {
            while Self::process_scroll_to_bottom_method(&me2, &sub).await {}
        }));

        let sub = node_ref.subscribe_method_call("receive").unwrap();
        let me2 = self.me.clone();
        tasks
            .push(ex.spawn(async move { while Self::process_receive_method(&me2, &sub).await {} }));

        let animator = {
            let me = self.me.clone();
            let cv = self.motion_cv.clone();
            ex.spawn(async move { Self::run_animator(&me, cv).await })
        };
        tasks.push(animator);

        // The single background loading pipeline.
        let loader_task = {
            let loader = self.loader.clone();
            ex.spawn(async move { loader.run().await })
        };
        tasks.push(loader_task);

        // Message lifecycle methods live on the type sub-nodes.
        let privmsg_node = self.types().privmsg.node().upgrade().unwrap();

        let sub = privmsg_node.subscribe_method_call("insert_line").unwrap();
        let me2 = self.me.clone();
        tasks.push(ex.spawn(async move {
            while Self::process_insert_line_method(&me2, &sub, true).await {}
        }));

        let sub = privmsg_node.subscribe_method_call("insert_unconf_line").unwrap();
        let me2 = self.me.clone();
        tasks.push(ex.spawn(async move {
            while Self::process_insert_line_method(&me2, &sub, false).await {}
        }));

        let sub = privmsg_node.subscribe_method_call("confirm").unwrap();
        let me2 = self.me.clone();
        tasks
            .push(ex.spawn(async move { while Self::process_confirm_method(&me2, &sub).await {} }));

        // File status updates arrive on the filemsg type node.
        let filemsg_node = self.types().filemsg.node().upgrade().unwrap();
        let sub = filemsg_node.subscribe_method_call("set_file_status").unwrap();
        let me2 = self.me.clone();
        tasks.push(
            ex.spawn(
                async move { while Self::process_set_file_status_method(&me2, &sub).await {} },
            ),
        );

        let mut on_modify = OnModify::new(ex, self.node.clone(), self.me.clone());
        on_modify.when_change_external(self.rect.prop(), |self_, _| async move {
            self_.redraw.trigger();
        });
        i!("chatview start: {} tasks spawned", tasks.len());

        // Styling changes re-wrap: the reflow protocol. Every shared
        // styling property that shapes text (and the privmsg node's
        // own cap) triggers it; color-only changes ride along since
        // layouts embed their colors.
        for name in [
            "font_size",
            "timestamp_font_size",
            "timestamp_width",
            "line_height",
            "message_spacing",
            "baseline",
            "timestamp_color",
            "text_color",
        ] {
            let Some(prop) = node_ref.get_property(name) else { continue };
            on_modify.when_change_external(prop, |self_, _| async move {
                self_.reflow().await;
            });
        }
        for name in ["cap_max_height", "nick_colors", "action_text_color", "url_text_color"] {
            let Some(prop) = self.types().privmsg.node().upgrade().unwrap().get_property(name)
            else {
                continue
            };
            on_modify.when_change_external(prop, |self_, _| async move {
                self_.reflow().await;
            });
        }
        for name in ["font_size", "color"] {
            let Some(prop) = self.types().datemsg.node().upgrade().unwrap().get_property(name)
            else {
                continue
            };
            on_modify.when_change_external(prop, |self_, _| async move {
                self_.reflow().await;
            });
        }
        tasks.append(&mut on_modify.tasks);

        *self.tasks.lock() = tasks;
    }

    fn stop(&self) {
        self.tasks.lock().clear();
        self.motion_cv.notify();
        self.buffer.lock_blocking().clear();
    }

    async fn draw(
        &self,
        parent_rect: Rectangle,
        atom: &mut PropertyAtomicGuard,
    ) -> Option<DrawUpdate> {
        let prev_rect = self.rect.get();
        self.rect.eval(atom, &parent_rect).ok()?;
        let rect = self.rect.get();
        let rect_changed = rect != prev_rect;

        let mut buffer = self.buffer.lock().await;
        let total = buffer.total_height();
        let scroll = {
            let mut ctl = self.controller.lock();
            ctl.set_content(total, rect.h);
            ctl.scroll()
        };

        // Deferred scroll restore: resolves once coverage reaches the
        // saved anchor (returns true when it just resolved).
        if self.try_pending_restore(&buffer, rect.h) {
            t!("scroll restore resolved");
        }

        // The visible window: geometry is O(log n); only the visible
        // records get draw work.
        let range = buffer.visible_range(scroll, rect.h);
        t!("draw scroll={scroll} total={total} visible={range:?}");

        let types = self.types().clone();
        let selected = self.selected.lock().clone();
        let hi_bg_color: Color = self.hi_bg_color.get();
        let buffer_len = buffer.len();
        let range_start = range.start;
        let range_end = range.end;
        let mut instrs = vec![DrawInstruction::ApplyView(rect)];
        let mut clipped_calls: Vec<(u64, DrawCall)> = vec![];
        let mut geometry_changed = false;
        for idx in range {
            let Some(rec_key) = buffer.record_at(idx).map(|rec| (rec.ts, rec.id)) else { continue };
            let rec_id = rec_key.1;
            let Some(top) = buffer.pos_of_key(&rec_key) else { continue };
            let is_selected = selected.contains(&rec_key);

            // Materialize and measure; a height that drifted from the
            // record's stored value (the layout re-wrapped at a
            // different width or styling) flows back into geometry
            // with scroll compensation.
            let height = {
                let Some(rec) = buffer.record_at(idx) else { continue };
                types.ensure_materialized(rec)
            };
            let stored = buffer.index_get_height(&rec_key).unwrap_or(0.);
            if (height - stored).abs() > 0.05 {
                if let Some(delta) = buffer.set_height_key(&rec_key, height) {
                    let mut ctl = self.controller.lock();
                    ctl.compensate(delta, top <= scroll);
                    drop(ctl);
                    geometry_changed = true;
                    t!("height drift id={rec_id} {stored} -> {height}");
                }
            }

            let outcome = {
                let Some(rec) = buffer.record_at(idx) else { continue };
                types.draw(rec, &self.renderer)
            };
            let off_y = scroll + rect.h - top;

            // Chatview-drawn selection highlight behind the type's
            // instructions: types stay ignorant of selection.
            if is_selected {
                instrs.push(DrawInstruction::SetPos(Point::new(0., off_y)));
                let mut mesh = MeshBuilder::new(gfxtag!("chatview_selbg"));
                mesh.draw_filled_box(&Rectangle::new(0., 0., rect.w, height), hi_bg_color);
                instrs.push(DrawInstruction::Draw(mesh.alloc(&self.renderer).draw_untextured()));
            }

            match outcome {
                msg::DrawOutcome::Inline(minstrs) => {
                    instrs.push(DrawInstruction::SetPos(Point::new(0., off_y)));
                    instrs.extend(minstrs);
                }
                msg::DrawOutcome::Clipped { instrs: minstrs, clip_h } => {
                    // Collapsed long message: its own sibling draw call
                    // so the clip dies with the call's frame.
                    let mut cap_instrs = vec![
                        DrawInstruction::ApplyView(rect),
                        DrawInstruction::SetPos(Point::new(0., off_y)),
                        DrawInstruction::ApplyView(Rectangle::new(0., 0., rect.w, clip_h)),
                    ];
                    cap_instrs.extend(minstrs);
                    clipped_calls.push((
                        self.dc_key ^ (clipped_calls.len() as u64 + 1),
                        DrawCall::new(cap_instrs, vec![], self.z_index.get(), "chatview_cap"),
                    ));
                }
            }
        }
        drop(buffer);
        if geometry_changed {
            // One more pass with the corrected geometry.
            self.redraw.trigger();
        }

        // Feed the loader: the coverage it must maintain and the wakes
        // for approaching the top of coverage and for rect changes.
        self.loader.update_viewport(scroll, rect.h);
        if total - (scroll + rect.h) < rect.h {
            self.loader.wake(Wakeup::NearTop);
        }
        if rect_changed {
            self.loader.wake(Wakeup::RectChange);
            t!("rect changed to {rect:?}");
        }

        if let Some(toast) = self.link_toast.lock().as_ref() {
            // Overlay defers with the cursor at emit time; reset it so
            // the toast's anchor resolves in view-local coordinates
            // instead of wherever the last message was drawn.
            instrs.push(DrawInstruction::SetPos(Point::new(0., 0.)));
            instrs.push(DrawInstruction::Overlay(toast.build_instrs(&self.renderer)));
        }

        let mut draw_calls =
            vec![(self.dc_key, DrawCall::new(instrs, vec![], self.z_index.get(), "chatview"))];
        draw_calls.extend(clipped_calls);

        Some(DrawUpdate { key: self.dc_key, draw_calls })
    }

    async fn handle_key_down(&self, key: KeyCode, _mods: KeyMods, repeat: bool) -> bool {
        if repeat {
            return false
        }

        let rect = self.rect.get();
        match key {
            KeyCode::PageUp => {
                let mut ctl = self.controller.lock();
                ctl.page_tick(1., rect.h / 2.);
                drop(ctl);
                self.notify_motion();
                true
            }
            KeyCode::PageDown => {
                let mut ctl = self.controller.lock();
                ctl.page_tick(-1., rect.h / 2.);
                drop(ctl);
                self.notify_motion();
                true
            }
            _ => false,
        }
    }

    async fn handle_mouse_btn_down(&self, btn: MouseButton, mouse_pos: Point) -> bool {
        let rect = self.rect.get();
        if !rect.contains(mouse_pos) {
            return false
        }

        // Right-click on a URL: copy it and show the "Copied link" toast.
        if btn == MouseButton::Right {
            if let Some((_, msg::Hit::Url(url))) = self.content_hit(mouse_pos).await {
                self.show_toast(&url, mouse_pos - rect.pos()).await;
                return true
            }
            return false
        }

        if btn != MouseButton::Left {
            return false
        }

        // A left press on a URL or nick consumes the press so no
        // selection starts; the activation itself happens on mouse-up.
        if self.content_hit(mouse_pos).await.is_some() {
            return true
        }

        // Query whether the clicked line is already selected. We select
        // immediately only if it wasn't (for instant feedback). If it
        // was already selected we leave it and let mouse-up decide: a
        // stationary click toggles it off, but a drag keeps it selected.
        let was_selected = self.is_line_selected(mouse_pos.y).await;
        if !was_selected {
            self.select_line(mouse_pos.y).await;
        }
        *self.select_drag.lock() =
            Some(SelectDrag { down_y: mouse_pos.y, was_selected, dragged: false });
        self.mouse_btn_held.store(true, Ordering::Relaxed);
        true
    }

    async fn handle_mouse_btn_up(&self, btn: MouseButton, mouse_pos: Point) -> bool {
        if btn != MouseButton::Left {
            return false
        }
        self.mouse_btn_held.store(false, Ordering::Relaxed);

        // URL/nick activation happens on mouse-up (the press was
        // already consumed so no selection gesture is in flight).
        if self.rect.get().contains(mouse_pos) && self.content_hit(mouse_pos).await.is_some() {
            return self.dispatch_content_hit(mouse_pos).await
        }

        // A stationary click on an already-selected line deselects it. A
        // drag (or a click on an unselected line) leaves selection as-is.
        let drag = self.select_drag.lock().take();
        if let Some(d) = drag {
            if !d.dragged && d.was_selected && self.rect.get().contains(mouse_pos) {
                self.deselect_line(mouse_pos.y).await;
            }
        }

        false
    }

    async fn handle_mouse_move(&self, mouse_pos: Point) -> bool {
        // Stored for use in handle_mouse_wheel()
        *self.mouse_pos.lock() = mouse_pos.clone();

        if !self.mouse_btn_held.load(Ordering::Relaxed) {
            return false
        }
        if !self.rect.get().contains(mouse_pos) {
            return false
        }

        // Dragging only ever selects. Once the mouse moves past the
        // threshold we latch `dragged` so the upcoming mouse-up won't
        // treat this as a toggling click.
        let dragged = {
            let mut drag = self.select_drag.lock();
            if let Some(d) = drag.as_mut() {
                if (mouse_pos.y - d.down_y).abs() > SELECT_DRAG_THRESHOLD {
                    d.dragged = true;
                }
                d.dragged
            } else {
                false
            }
        };
        if dragged {
            self.select_line(mouse_pos.y).await;
        }

        false
    }

    async fn handle_mouse_wheel(&self, wheel_pos: Point) -> bool {
        let rect = self.rect.get();

        let mouse_pos = self.mouse_pos.lock().clone();
        if !rect.contains(mouse_pos) {
            return false
        }

        let mut ctl = self.controller.lock();
        // The raw delta is the tick count: mouse notches deliver ±1.0,
        // while trackpads stream small fractional axis values that must
        // scale proportionally. wheel_page_frac sets the viewport
        // fraction scrolled per notch (0.5 = half a page).
        ctl.page_tick(wheel_pos.y, rect.h * self.wheel_page_frac.get());
        drop(ctl);
        self.notify_motion();
        true
    }

    fn gesture_set(&self) -> GestureSet {
        GestureSet::CHATVIEW
    }

    fn gesture_hit_test(&self, pos: Point) -> bool {
        self.rect.get().contains(pos)
    }

    async fn handle_gesture(&self, gesture: GestureAction) -> bool {
        match gesture {
            GestureAction::Down { .. } => {
                // A new touch resets the long-press mode and cancels any
                // in-flight motion, before the touch even passes the slop.
                *self.lp_mode.lock() = LongPressMode::None;
                self.controller.lock().touch_down();
                true
            }
            GestureAction::DragStart { start } => {
                // A grab owns the scroll 1:1.
                self.controller.lock().drag_start(start.y);
                true
            }
            GestureAction::DragMove { curr, .. } => {
                // Extending a long-press selection instead of scrolling.
                if *self.lp_mode.lock() == LongPressMode::Select {
                    self.select_line(curr.y).await;
                    return true
                }

                self.controller.lock().drag_move(curr.y);
                self.sync_is_at_bottom();
                self.redraw.trigger();
                true
            }
            GestureAction::DragEnd { vel, .. } => {
                // No inertia while a selection is being extended.
                if *self.lp_mode.lock() == LongPressMode::Select {
                    return true
                }

                // Flick on the session's release velocity.
                self.controller.lock().drag_end(vel.y);
                self.notify_motion();
                true
            }
            GestureAction::Up { .. } => true,
            GestureAction::Tap { pos } => {
                self.handle_tap(pos).await;
                true
            }
            GestureAction::LongPress { pos } => {
                self.handle_long_press(pos).await;
                true
            }
        }
    }
}

/// Copy text for the selected records in display order, joined by
/// newlines; each record contributes the text its type defines.
pub(crate) fn copy_selected(
    buffer: &MsgBuffer,
    selected: &HashSet<(Timestamp, MessageId)>,
    types: &msg::TypeNodes,
) -> String {
    let mut lines = vec![];
    for rec in buffer.iter_display_order() {
        if selected.contains(&(rec.ts, rec.id)) {
            if let Some(text) = types.copy_text(rec) {
                lines.push(text);
            }
        }
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::node::create_chatview;

    /// Every property the chatview wraps must exist on the factory's
    /// node — catches factory/impl drift that would panic at runtime
    /// (a missing prop only explodes when the screen is built).
    #[test]
    fn factory_declares_all_wrapped_properties() {
        let node = create_chatview("chatview");
        let node = node.setup_null();
        for name in [
            "channel",
            "rect",
            "font_size",
            "timestamp_font_size",
            "timestamp_width",
            "line_height",
            "message_spacing",
            "baseline",
            "timestamp_color",
            "text_color",
            "hi_bg_color",
            "is_at_bottom",
            "wheel_page_frac",
            "z_index",
            "priority",
        ] {
            assert!(node.get_property(name).is_some(), "chatview factory missing '{name}'");
        }
    }

    use crate::{
        prop::{Property, PropertyAtomicGuard, PropertySubType, PropertyType},
        scene::{SceneNode as TestSceneNode, SceneNodeType as TestSceneNodeType},
        ui::RedrawTrigger as TestRedrawTrigger,
    };

    /// Selection copy text follows display order (newest first), each
    /// record contributing its type's copy text, joined by newlines —
    /// including derived date separators.
    #[test]
    fn copy_selected_follows_display_order() {
        let chat = create_chatview("chatview");
        let chat = chat.setup_null();
        let atom = &mut PropertyAtomicGuard::none();
        chat.set_property_f32(atom, Role::App, "font_size", 14.).unwrap();
        chat.set_property_f32(atom, Role::App, "timestamp_font_size", 10.).unwrap();
        chat.set_property_f32(atom, Role::App, "timestamp_width", 50.).unwrap();
        chat.set_property_f32(atom, Role::App, "line_height", 20.).unwrap();
        chat.set_property_f32(atom, Role::App, "message_spacing", 4.).unwrap();
        let prop = chat.get_property("rect").unwrap();
        prop.set_f32(atom, Role::App, 0, 0.).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.).unwrap();
        prop.set_f32(atom, Role::App, 2, 800.).unwrap();
        prop.set_f32(atom, Role::App, 3, 600.).unwrap();
        let prop = chat.get_property("text_color").unwrap();
        for (i, c) in [1., 1., 1., 1.].iter().enumerate() {
            prop.set_f32(atom, Role::App, i, *c).unwrap();
        }
        let prop = chat.get_property("timestamp_color").unwrap();
        for (i, c) in [0.5, 0.5, 0.5, 1.].iter().enumerate() {
            prop.set_f32(atom, Role::App, i, *c).unwrap();
        }

        let mut wscale = TestSceneNode::new("w", TestSceneNodeType::Object);
        wscale
            .add_property(Property::new("scale", PropertyType::Float32, PropertySubType::Null))
            .unwrap();
        let wscale = wscale.setup_null();
        wscale.set_property_f32(atom, Role::App, "scale", 1.).unwrap();
        let window_scale = PropertyFloat32::wrap(&wscale, Role::Internal, "scale", 0).unwrap();

        let shared = msg::SharedProps::wrap(&chat, window_scale);
        let mut raw = MsgBuffer::new();
        let buffer = Arc::new(AsyncMutex::new(raw));
        let (redraw, _rx) = TestRedrawTrigger::new();
        let loader = Loader::new(buffer.clone(), redraw);
        let chat_weak: Weak<ChatView> = Weak::new();

        let types = smol::block_on(async {
            let i18n = crate::util::i18n::I18nBabelFish::new(String::new(), "en-US");
            msg::TypeNodes::new(&chat, shared, i18n, loader, buffer.clone(), chat_weak).await
        });

        // Two days of privmsgs; the buffer derives the separator.
        let ts = |day: i64, h: u32| {
            use chrono::TimeZone;
            let date =
                chrono::NaiveDate::from_ymd_opt(2026, 8, 30).unwrap() + chrono::Duration::days(day);
            let dt = date.and_hms_opt(h, 0, 0).unwrap();
            chrono::Local.from_local_datetime(&dt).unwrap().timestamp_millis() as Timestamp
        };

        let mut buffer = smol::block_on(buffer.lock());
        let mut selected = HashSet::new();
        let mut ids = vec![];
        for (ts, idb, text) in
            [(ts(0, 10), b'a', "oldest"), (ts(0, 11), b'b', "middle"), (ts(1, 9), b'c', "newest")]
        {
            let mut id = [0u8; 32];
            id[0] = idb;
            let id = MessageId(id);
            ids.push(id);
            let payload = codec::encode_privmsg_payload("alice", text, true);
            let rec = MsgRecord { ts, id, msg_type: MsgType::PrivMsg, payload, height: 0. };
            types.measure(&rec);
            assert!(buffer.insert(MsgRecord { height: 20., ..rec }));
        }

        // The derived separator for day 1 sits between the days.
        let sep = buffer
            .iter_display_order()
            .find(|rec| rec.msg_type.is_derived())
            .expect("separator derived")
            .clone();
        types.measure(&sep);

        selected.insert((ts(0, 10), ids[0]));
        selected.insert((ts(1, 9), ids[2]));
        selected.insert((sep.ts, sep.id));

        // Display order (newest first): c, [sep], b, a — selected c,
        // sep, a contribute in that order.
        let text = copy_selected(&buffer, &selected, &types);
        let label = msg::datemsg::datestr(sep.ts);
        assert_eq!(text, format!("alice newest\n{label}\nalice oldest"));

        // Nothing selected: nothing copied.
        selected.clear();
        assert_eq!(copy_selected(&buffer, &selected, &types), "");
    }
}

impl Drop for ChatView {
    fn drop(&mut self) {
        self.renderer.replace_draw_calls(vec![(self.dc_key, Default::default())]);
    }
}

impl std::fmt::Debug for ChatView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.node.upgrade().unwrap())
    }
}
