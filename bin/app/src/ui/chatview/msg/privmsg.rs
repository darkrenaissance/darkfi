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

//! The privmsg type node: insertion semantics and basic rendering.
//!
//! One scene sub-node (created with the chatview, stable across
//! channels) carries the type's properties, signals, and lifecycle
//! methods; per-id instances carry data + rendered state. Insertion
//! persists through the loader (the sole kvdb writer), dedups by the
//! composite key, and confirms unconfirmed messages in place. The
//! rendered state is a pure cache of (data, props): a signature check
//! skips re-layouts, and regen drops it for a full rebuild.

use async_lock::Mutex as AsyncMutex;
use async_trait::async_trait;
use chrono::{Local, TimeZone};
use darkfi_serial::{Decodable, Encodable};
use parking_lot::Mutex as SyncMutex;
use std::{
    collections::{HashMap, HashSet},
    hash::{DefaultHasher, Hash, Hasher},
    io::Cursor,
    sync::{Arc, Weak},
};
use url::Url;

use crate::{
    gfx::{gfxtag, DrawInstruction, EpochTracker, Point, Rectangle, Renderer},
    mesh::{Color, MeshBuilder},
    prop::{
        Property, PropertyAtomicGuard, PropertyColor, PropertyFloat32, PropertyPermission,
        PropertyPtr, PropertyStr, PropertySubType, PropertyType, Role,
    },
    scene::{CallArgType, Pimpl, SceneNode, SceneNodePtr, SceneNodeType, SceneNodeWeak},
    text,
    ui::{
        chatview::{
            buffer::MsgBuffer, codec, loader::Loader, ChatView, MessageId, MsgRecord, MsgType,
            Timestamp, Wakeup,
        },
        UIObject,
    },
    ExecutorPtr,
};

use super::{evict_beyond, filemsg::get_file_url, DrawOutcome, Hit, SharedProps};

macro_rules! t { ($($arg:tt)*) => { trace!(target: "ui::chatview::privmsg", $($arg)*); } }

/// Instance map key: the composite record key (derived records share
/// synthetic ids).
pub type InstKey = (Timestamp, MessageId);

/// Unconfirmed bodies render gray.
const UNCONF_COLOR: [f32; 4] = [0.4, 0.4, 0.4, 1.];

/// IRC CTCP ACTION framing prefix, e.g. `\x01ACTION waves\x01`.
const CTCP_ACTION_PREFIX: &str = "\u{1}ACTION ";

/// If `text` is a CTCP ACTION-framed message body, return the stripped
/// action text. A single trailing `\x01` delimiter is stripped when
/// present but is not required, since bodies truncated by length
/// limits may lose it.
fn parse_ctcp_action(text: &str) -> Option<&str> {
    let body = text.strip_prefix(CTCP_ACTION_PREFIX)?;
    let body = body.strip_suffix('\u{1}').unwrap_or(body);
    Some(body)
}

fn is_notice(nick: &str) -> bool {
    nick == "NOTICE"
}

/// Stable per-nick color: hash the nick, index into the palette. An
/// empty palette falls back to white.
fn select_nick_color(nick: &str, nick_colors: &[Color]) -> Color {
    if nick_colors.is_empty() {
        return [1., 1., 1., 1.]
    }
    let mut hasher = DefaultHasher::new();
    nick.hash(&mut hasher);
    let i = hasher.finish() as usize;
    nick_colors[i % nick_colors.len()]
}

fn read_nick_colors(prop: &PropertyPtr) -> Vec<Color> {
    let mut colors = vec![];
    let mut color = [0f32; 4];
    for i in 0..prop.get_len() {
        color[i % 4] = prop.get_f32(i).expect("prop logic err");
        if i > 0 && i % 4 == 0 {
            let color = std::mem::take(&mut color);
            colors.push(color);
        }
    }
    colors
}

fn gen_timestr(timestamp: Timestamp) -> String {
    let Some(dt) = Local.timestamp_millis_opt(timestamp as i64).single() else {
        return String::new()
    };
    dt.format("%H:%M").to_string()
}

/// The type-specific property handles, wrapped off this node.
#[derive(Clone)]
pub struct PrivOwnProps {
    pub nick_colors: PropertyPtr,
    pub action_text_color: PropertyColor,
    pub url_text_color: PropertyColor,
    pub url_bg_color: PropertyColor,
    pub url_bg_border_size: PropertyFloat32,
    pub url_bg_border_color: PropertyColor,
    pub cap_max_height: PropertyFloat32,
    /// Channel label styling (the room-name span).
    pub channel_color: PropertyColor,
    pub channel_font_size: PropertyFloat32,
    /// Spacing between timestamp, chip, and line.
    pub channel_spacing: PropertyFloat32,
    pub channel_bg_color: PropertyColor,
    pub channel_border_size: PropertyFloat32,
    pub channel_border_color: PropertyColor,
}

/// Everything a materialized instance needs to render: data decoded
/// from the record payload, plus a pure cache of (data, props).
pub struct PrivMsg {
    /// Decoded payload state.
    pub data: PrivData,
    /// The payload the rendered state was built from (data changes for
    /// a live id — e.g. confirmation — invalidate the cache).
    payload: Vec<u8>,
    /// Signature the rendered state was built with.
    sig: LayoutSig,
    txt_layout: text::TextLayout,
    ts_layout: text::TextLayout,
    /// The channel span's layout, when rendered (show_channel active).
    chan_layout: Option<text::TextLayout>,
    /// The channel chip rect (label bounds + padding for bg/border),
    /// when the span is rendered.
    channel_chip: Option<Rectangle>,
    instrs: Option<Vec<DrawInstruction>>,
    /// URL hit rects in message-local coordinates, tagged with their
    /// URL string.
    url_rects: Vec<(String, Rectangle)>,
    /// Nick-prefix hit rects in message-local coordinates, tagged with
    /// the nick.
    nick_rects: Vec<(String, Rectangle)>,
    /// The channel span's hit rect (the label's row, after the
    /// timestamp), when rendered.
    channel_rect: Option<Rectangle>,
    /// The expand/collapse affordance hit rect, when the message is
    /// over the cap.
    affordance_rect: Option<Rectangle>,
    /// The un-capped text height.
    full_text_height: f32,
    /// Whether the message's full height exceeds the cap.
    over_cap: bool,
    /// Whether the message is currently drawn collapsed.
    collapsed: bool,
    /// Measured height incl. message spacing.
    pub height: f32,
}

pub struct PrivData {
    pub ts: Timestamp,
    pub id: MessageId,
    pub nick: String,
    pub text: String,
    pub confirmed: bool,
    /// The channel the message belongs to (from the payload); rendered
    /// as the channel span when channel display is active.
    pub channel: String,
    /// IRC-style CTCP ACTION (`/me`); `text` holds the stripped body.
    pub is_action: bool,
    pub is_notice: bool,
    pub expanded: bool,
}

impl PrivData {
    /// The full rendered line text: NOTICE renders the body alone,
    /// normal messages render "<nick> <body>", actions
    /// "* <nick> <body>".
    pub fn line_text(&self) -> String {
        if self.is_notice {
            return self.text.clone()
        }
        if self.is_action {
            return format!("* {} {}", self.nick, self.text)
        }
        format!("{} {}", self.nick, self.text)
    }

    /// Byte offset of the body within the rendered line text. This is
    /// also the end of the nick-colored prefix.
    pub fn body_offset(&self) -> usize {
        if self.is_notice {
            return 0
        }
        if self.is_action {
            // "* " + nick + " "
            return self.nick.len() + 3
        }
        self.nick.len() + 1
    }
}

/// The inputs a rendered state depends on; any mismatch forces a
/// re-layout. Colors are included so palette/role changes invalidate.
#[derive(PartialEq)]
struct LayoutSig {
    width: f32,
    font_size: f32,
    timestamp_font_size: f32,
    line_height: f32,
    window_scale: f32,
    confirmed: bool,
    expanded: bool,
    show_channel: bool,
    body_color: Color,
    nick_color: Color,
    ts_color: Color,
    channel_color: Color,
    /// The channel label's font size (timestamp size when inherited).
    channel_font_size: f32,
    /// Spacing between timestamp, chip, and line (auto fraction of
    /// the channel font size when the property is 0).
    channel_spacing: f32,
    channel_bg_color: Color,
    channel_border_color: Color,
    channel_border_size: f32,
}

struct PrivInner {
    instances: HashMap<InstKey, PrivMsg>,
    /// Last-access counter per instance, for LRU eviction.
    touches: HashMap<InstKey, u64>,
    /// Monotonic access counter driving `touches`.
    access: u64,
    /// Epoch-scoped mesh caches die with the epoch.
    epoch_tracker: Option<EpochTracker>,
    /// How many layouts have been (re)built; cache-behavior test hook.
    layout_builds: usize,
}

pub type PrivMsgNodePtr = Arc<PrivMsgNode>;

/// The privmsg type node.
pub struct PrivMsgNode {
    node: SceneNodeWeak,
    shared: SharedProps,
    own: PrivOwnProps,
    loader: Arc<Loader>,
    buffer: Arc<AsyncMutex<MsgBuffer>>,
    chat: Weak<ChatView>,
    inner: SyncMutex<PrivInner>,
    /// Messages the user expanded (over-cap ones collapse by default);
    /// survives rendered-state regens.
    expanded: SyncMutex<HashSet<InstKey>>,
    /// "Copied link" overlay styling/content; the chatview-drawn toast
    /// reads these through the getters below.
    url_copy_text: PropertyStr,
    url_copy_fg_color: PropertyColor,
    url_copy_bg_color: PropertyColor,
    url_copy_font_size: PropertyFloat32,
    url_copy_padding: PropertyFloat32,
    url_copy_offset: PropertyFloat32,
    url_copy_duration: PropertyFloat32,
}

impl PrivMsgNode {
    pub async fn new(
        node: SceneNodeWeak,
        shared: SharedProps,
        loader: Arc<Loader>,
        buffer: Arc<AsyncMutex<MsgBuffer>>,
        chat: Weak<ChatView>,
    ) -> Pimpl {
        let node_ref = &node.upgrade().unwrap();
        let nick_colors = node_ref.get_property("nick_colors").expect("privmsg nick_colors");
        let action_text_color =
            PropertyColor::wrap(node_ref, Role::Internal, "action_text_color").unwrap();
        let url_text_color =
            PropertyColor::wrap(node_ref, Role::Internal, "url_text_color").unwrap();
        let url_bg_color = PropertyColor::wrap(node_ref, Role::Internal, "url_bg_color").unwrap();
        let url_bg_border_size =
            PropertyFloat32::wrap(node_ref, Role::Internal, "url_bg_border_size", 0).unwrap();
        let url_bg_border_color =
            PropertyColor::wrap(node_ref, Role::Internal, "url_bg_border_color").unwrap();
        let cap_max_height =
            PropertyFloat32::wrap(node_ref, Role::Internal, "cap_max_height", 0).unwrap();
        let channel_color = PropertyColor::wrap(node_ref, Role::Internal, "channel_color").unwrap();
        let channel_font_size =
            PropertyFloat32::wrap(node_ref, Role::Internal, "channel_font_size", 0).unwrap();
        let channel_spacing =
            PropertyFloat32::wrap(node_ref, Role::Internal, "channel_spacing", 0).unwrap();
        let channel_bg_color =
            PropertyColor::wrap(node_ref, Role::Internal, "channel_bg_color").unwrap();
        let channel_border_size =
            PropertyFloat32::wrap(node_ref, Role::Internal, "channel_border_size", 0).unwrap();
        let channel_border_color =
            PropertyColor::wrap(node_ref, Role::Internal, "channel_border_color").unwrap();
        let url_copy_text =
            PropertyStr::wrap(node_ref, Role::Internal, "url_copy_text", 0).unwrap();
        let url_copy_fg_color =
            PropertyColor::wrap(node_ref, Role::Internal, "url_copy_fg_color").unwrap();
        let url_copy_bg_color =
            PropertyColor::wrap(node_ref, Role::Internal, "url_copy_bg_color").unwrap();
        let url_copy_font_size =
            PropertyFloat32::wrap(node_ref, Role::Internal, "url_copy_font_size", 0).unwrap();
        let url_copy_padding =
            PropertyFloat32::wrap(node_ref, Role::Internal, "url_copy_padding", 0).unwrap();
        let url_copy_offset =
            PropertyFloat32::wrap(node_ref, Role::Internal, "url_copy_offset", 0).unwrap();
        let url_copy_duration =
            PropertyFloat32::wrap(node_ref, Role::Internal, "url_copy_duration", 0).unwrap();

        let self_ = Arc::new(Self {
            node: node.clone(),
            shared,
            own: PrivOwnProps {
                nick_colors,
                action_text_color,
                url_text_color,
                url_bg_color,
                url_bg_border_size,
                url_bg_border_color,
                cap_max_height,
                channel_color,
                channel_font_size,
                channel_spacing,
                channel_bg_color,
                channel_border_size,
                channel_border_color,
            },
            loader,
            buffer,
            chat,
            inner: SyncMutex::new(PrivInner {
                instances: HashMap::new(),
                touches: HashMap::new(),
                access: 0,
                epoch_tracker: None,
                layout_builds: 0,
            }),
            expanded: SyncMutex::new(HashSet::new()),
            url_copy_text,
            url_copy_fg_color,
            url_copy_bg_color,
            url_copy_font_size,
            url_copy_padding,
            url_copy_offset,
            url_copy_duration,
        });
        Pimpl::PrivMsgNode(self_)
    }

    /// Read the current layout signature off the live properties.
    fn current_sig(&self, data: &PrivData) -> LayoutSig {
        // The body wraps at the full view width; the first line is
        // indented past the gutter via the layout's text-indent.
        let width = self.shared.rect.get().w;
        let font_size = if data.is_notice {
            self.shared.font_size.get() * 0.8
        } else {
            self.shared.font_size.get()
        };
        let nick_colors = read_nick_colors(&self.own.nick_colors);
        let nick_color = select_nick_color(&data.nick, &nick_colors);
        let body_color = if data.is_action {
            if data.confirmed {
                self.own.action_text_color.get()
            } else {
                UNCONF_COLOR
            }
        } else if data.confirmed {
            self.shared.text_color.get()
        } else {
            UNCONF_COLOR
        };
        LayoutSig {
            width,
            font_size,
            timestamp_font_size: self.shared.timestamp_font_size.get(),
            line_height: self.shared.line_height.get(),
            window_scale: self.shared.window_scale.get(),
            confirmed: data.confirmed,
            expanded: data.expanded,
            show_channel: self.shared.show_channel.get(),
            body_color,
            nick_color,
            ts_color: self.shared.timestamp_color.get(),
            channel_color: self.own.channel_color.get(),
            channel_font_size: self.own.channel_font_size.get(),
            channel_spacing: self.own.channel_spacing.get(),
            channel_bg_color: self.own.channel_bg_color.get(),
            channel_border_color: self.own.channel_border_color.get(),
            channel_border_size: self.own.channel_border_size.get(),
        }
    }

    /// Build the instance's rendered state from live props + data.
    fn build_rendered(&self, key: &InstKey, data: PrivData) -> PrivMsg {
        let expanded = self.expanded.lock().contains(key);
        let data = PrivData { expanded, ..data };
        let sig = self.current_sig(&data);
        let linetext = data.line_text();
        let line_height = self.shared.line_height.get();
        let window_scale = self.shared.window_scale.get();

        let mut foreground_colors = vec![];
        if !data.is_notice {
            foreground_colors.push((0..data.body_offset(), sig.nick_color));
        }
        foreground_colors.extend(url_color_ranges(
            &data.text,
            data.body_offset(),
            self.own.url_text_color.get(),
        ));

        let timestr = gen_timestr(data.ts);
        let ts_layout = text::make_layout(
            &timestr,
            sig.ts_color,
            sig.timestamp_font_size,
            line_height / sig.timestamp_font_size,
            window_scale,
            None,
            &[],
        );
        // The spacing property governs every junction on the first
        // line: timestamp → line when the label is hidden, and
        // timestamp → chip → line when it is shown. Wrapped lines
        // return to the view's left edge.
        let gap = sig.channel_spacing;
        let (chan_layout, channel_rect, channel_chip, body_indent) = if sig.show_channel &&
            !data.channel.is_empty()
        {
            let chan_x = ts_layout.width() + gap;
            // Keep an absurd room name from eating the line.
            let max_chan_w = (self.shared.rect.get().w * 0.3).max(sig.channel_font_size);
            let label =
                ellipsize_to_width(&data.channel, max_chan_w, sig.channel_font_size, window_scale);
            let layout = text::make_layout(
                &label,
                sig.channel_color,
                sig.channel_font_size,
                line_height / sig.channel_font_size,
                window_scale,
                None,
                &[],
            );
            let rect = Rectangle::new(chan_x, 0., layout.width(), ts_layout.height());
            // The chip (bg + border) hugs the label's glyphs. The
            // bounds are in the label layout's LOCAL coordinates,
            // so shift them onto the label's position on the
            // timestamp row before padding.
            let pad = sig.channel_border_size + sig.channel_font_size * 0.2;
            let bounds = layout_glyph_bounds(&layout);
            let chip =
                inflate_rect(&Rectangle::new(bounds.x + chan_x, bounds.y, bounds.w, bounds.h), pad);
            let indent = chan_x + layout.width() + gap;
            (Some(layout), Some(rect), Some(chip), indent)
        } else {
            (None, None, None, ts_layout.width() + gap)
        };

        // The body wraps at the full view width; the first line is
        // indented past the gutter (and channel span) via the
        // layout's text-indent, continuation lines start at x=0.
        let txt_layout = text::make_layout_with_indent(
            &linetext,
            sig.body_color,
            sig.font_size,
            line_height / sig.font_size,
            window_scale,
            Some(sig.width),
            &[],
            if data.is_notice { &[] } else { &foreground_colors },
            parley::Alignment::Start,
            parley::OverflowWrap::Normal,
            body_indent,
        );

        // Hit rects: URL-colored and nick-colored glyph runs become
        // clickable regions in message-local coordinates. The body
        // layout spans the full width (the first line's indent is
        // internal), so run offsets are already view-relative.
        let url_color = self.own.url_text_color.get();
        let nick_color = sig.nick_color;
        let url_rects = Self::compute_hit_rects(
            &txt_layout,
            &data,
            url_color,
            &url_ranges_of(&data, url_color),
            |raw| sanitize_url(raw),
        );
        let nick_rects = if data.is_notice {
            vec![]
        } else {
            let nick_range = 0..data.body_offset();
            Self::compute_hit_rects(&txt_layout, &data, nick_color, &[nick_range], |raw| {
                Some(raw.trim_end().to_string())
            })
        };

        // Cap/expand: long messages collapse to the cap by default.
        let full_text_height = txt_layout.height();
        let cap = self.own.cap_max_height.get();
        let over_cap = cap > 0. && full_text_height > cap;
        let collapsed = over_cap && !expanded;
        let affordance_rect = over_cap.then(|| {
            let line_height = self.shared.line_height.get();
            let width = self.shared.rect.get().w;
            let y = if collapsed { cap - line_height } else { full_text_height - line_height };
            Rectangle::new(width - 60., y.max(0.), 60., line_height)
        });

        let text_height = if collapsed { cap } else { full_text_height };
        let height = text_height + self.shared.message_spacing.get();
        PrivMsg {
            data,
            payload: vec![],
            sig,
            txt_layout,
            ts_layout,
            chan_layout,
            channel_chip,
            instrs: None,
            url_rects,
            nick_rects,
            channel_rect,
            affordance_rect,
            full_text_height,
            over_cap,
            collapsed,
            height,
        }
    }

    /// The instance, materializing (or re-materializing) as needed.
    /// Returns the measured height.
    fn ensure_materialized(&self, rec: &MsgRecord) -> f32 {
        let key = (rec.ts, rec.id);
        let mut inner = self.inner.lock();
        inner.access += 1;
        let access = inner.access;
        inner.touches.insert(key, access);
        if let Some(inst) = inner.instances.get_mut(&key) {
            let sig = self.current_sig(&inst.data);
            if inst.sig == sig && inst.payload == rec.payload {
                return inst.height
            }
        }

        let (nick, text, confirmed, channel) =
            codec::decode_privmsg_payload(&rec.payload, rec.ts, &rec.id);
        let (is_action, text) = match parse_ctcp_action(&text) {
            Some(action) => (true, action.to_string()),
            None => (false, text),
        };
        let is_notice = is_notice(&nick);
        let data = PrivData {
            ts: rec.ts,
            id: rec.id,
            nick,
            text,
            confirmed,
            channel,
            is_action,
            is_notice,
            expanded: true,
        };
        let inst = self.build_rendered(&key, data);
        let height = inst.height;
        let inst = PrivMsg { payload: rec.payload.clone(), ..inst };
        inner.instances.insert(key, inst);
        inner.layout_builds += 1;
        t!("materialized id={} height={height}", rec.id);
        height
    }

    /// Measure a record: materialize if needed, return the height.
    pub fn measure(&self, rec: &MsgRecord) -> f32 {
        self.ensure_materialized(rec)
    }

    /// Whether the instance currently holds rendered state.
    pub fn is_materialized(&self, rec: &MsgRecord) -> bool {
        self.inner.lock().instances.contains_key(&(rec.ts, rec.id))
    }

    /// How many instances are materialized (LRU bookkeeping, tests).
    pub fn instance_count(&self) -> usize {
        self.inner.lock().instances.len()
    }

    /// Drop an instance's rendered state (eviction). Render-scoped
    /// tasks would be cancelled here; none exist yet.
    pub fn release(&self, key: &InstKey) {
        let mut inner = self.inner.lock();
        if inner.instances.remove(key).is_some() {
            inner.touches.remove(key);
            t!("released id={}", key.1);
        }
    }

    /// Drop every instance's rendered state.
    pub fn release_all(&self) {
        let mut inner = self.inner.lock();
        let count = inner.instances.len();
        inner.instances.clear();
        inner.touches.clear();
        t!("released all ({count})");
    }

    /// Rebuild rendered state from live props + current data.
    pub fn regen(&self, key: &InstKey) {
        self.release(key);
    }

    /// Rebuild every instance's rendered state.
    pub fn regen_all(&self) {
        self.release_all();
    }

    /// The instance's measured height, if materialized.
    pub fn height(&self, key: &InstKey) -> Option<f32> {
        self.inner.lock().instances.get(key).map(|inst| inst.height)
    }

    /// Release the out-of-window instances beyond the LRU budget:
    /// `keep` are window members, the `budget` most recently touched
    /// of the rest survive.
    pub fn sweep(&self, keep: &std::collections::HashSet<InstKey>, budget: usize) {
        let mut inner = self.inner.lock();
        let releases = evict_beyond(keep, &inner.touches, budget);
        drop(inner);
        for key in releases {
            self.release(&key);
        }
    }

    /// Renderer-bound draw instructions in message-local coordinates
    /// (y grows downward from the message's top edge). Mesh allocation
    /// happens here — the draw edge — and is cached until the epoch or
    /// the layout signature invalidates it. Collapsed long messages
    /// come back [`DrawOutcome::Clipped`] so the chatview emits
    /// them as sibling calls with their own view.
    /// Draw-path re-evaluation of the expr-bound own styling props
    /// (see ChatView::eval_style for rationale).
    pub fn eval_style(&self, atom: &mut PropertyAtomicGuard) {
        self.own.action_text_color.eval(atom).expect("action_text_color");
        self.own.url_text_color.eval(atom).expect("url_text_color");
        self.own.cap_max_height.eval(atom).expect("cap_max_height");
        self.own.channel_color.eval(atom).expect("channel_color");
        self.own.channel_font_size.eval(atom).expect("channel_font_size");
        self.own.channel_spacing.eval(atom).expect("channel_spacing");
        self.own.channel_bg_color.eval(atom).expect("channel_bg_color");
        self.own.channel_border_color.eval(atom).expect("channel_border_color");
        self.own.channel_border_size.eval(atom).expect("channel_border_size");
    }

    pub fn draw(&self, rec: &MsgRecord, renderer: &Renderer) -> super::DrawOutcome {
        let key = (rec.ts, rec.id);
        let mut inner = self.inner.lock();
        inner.access += 1;
        let access = inner.access;
        inner.touches.insert(key, access);
        let epoch_changed =
            inner.epoch_tracker.get_or_insert_with(|| EpochTracker::new(renderer)).changed();
        if epoch_changed {
            for inst in inner.instances.values_mut() {
                inst.instrs = None;
            }
        }

        let Some(inst) = inner.instances.get_mut(&key) else { return DrawOutcome::Inline(vec![]) };
        if inst.instrs.is_none() {
            // `Move` offsets are RELATIVE (they add to the draw
            // cursor), so the assembly below tracks the cursor
            // explicitly and emits deltas to each segment's absolute
            // message-local position.
            let mut cursor = Point::new(0., 0.);
            let mut move_to =
                |instrs: &mut Vec<DrawInstruction>, cursor: &mut Point, x: f32, y: f32| {
                    instrs.push(DrawInstruction::Move(Point::new(x - cursor.x, y - cursor.y)));
                    *cursor = Point::new(x, y);
                };

            // Timestamp at the message origin.
            let mut instrs =
                text::render_layout(&inst.ts_layout, renderer, gfxtag!("chatview_privmsg_ts"));

            // The body renders at the origin too: its layout spans the
            // full width and carries the first line's gutter indent
            // internally, so wrapped lines begin at the left edge.
            // (No Move needed — the cursor never left the origin.)
            if url_regex().is_match(&inst.data.text) {
                let bg_instrs = text::render_backgrounds(
                    &inst.txt_layout,
                    self.own.url_text_color.get(),
                    self.own.url_bg_color.get(),
                    self.own.url_bg_border_color.get(),
                    self.own.url_bg_border_size.get(),
                    renderer,
                    gfxtag!("chatview_privmsg_urlbg"),
                );
                instrs.extend(bg_instrs);
            }

            let text_instrs =
                text::render_layout(&inst.txt_layout, renderer, gfxtag!("chatview_privmsg_text"));
            instrs.extend(text_instrs);

            // The channel chip: bg fill + border behind the label,
            // drawn at the origin (absolute coords) so it sits under
            // the label's Move. Invisible parts are skipped by the
            // same alpha/size rules the URL chip uses.
            if let (Some(chan_layout), Some(rect), Some(chip)) =
                (&inst.chan_layout, &inst.channel_rect, &inst.channel_chip)
            {
                let bg_color = self.own.channel_bg_color.get();
                let border_color = self.own.channel_border_color.get();
                let border_size = self.own.channel_border_size.get();
                if bg_color[3] > 0.01 || (border_size > 0.01 && border_color[3] > 0.01) {
                    let mut mesh = MeshBuilder::new(gfxtag!("chatview_privmsg_chanbg"));
                    if bg_color[3] > 0.01 {
                        mesh.draw_filled_box(chip, bg_color);
                    }
                    if border_size > 0.01 && border_color[3] > 0.01 {
                        mesh.draw_outline(chip, border_color, border_size);
                    }
                    instrs.push(DrawInstruction::Draw(mesh.alloc(renderer).draw_untextured()));
                }
                move_to(&mut instrs, &mut cursor, rect.x, 0.);
                let chan_instrs =
                    text::render_layout(chan_layout, renderer, gfxtag!("chatview_privmsg_chan"));
                instrs.extend(chan_instrs);
            }

            // The expand/collapse affordance label, centered in its rect.
            if let Some(rect) = &inst.affordance_rect {
                let label = if inst.collapsed { "+" } else { "-" };
                let label_layout = text::make_layout(
                    label,
                    self.shared.text_color.get(),
                    self.shared.font_size.get() * 0.8,
                    1.,
                    self.shared.window_scale.get(),
                    None,
                    &[],
                );
                move_to(
                    &mut instrs,
                    &mut cursor,
                    rect.x + (rect.w - label_layout.width()) / 2.,
                    rect.y + (rect.h - label_layout.height()) / 2.,
                );
                let label_instrs =
                    text::render_layout(&label_layout, renderer, gfxtag!("chatview_privmsg_cap"));
                instrs.extend(label_instrs);
            }

            inst.instrs = Some(instrs);
        }
        let instrs = inst.instrs.clone().unwrap_or_default();
        if inst.collapsed {
            let clip_h = inst.height - self.shared.message_spacing.get();
            DrawOutcome::Clipped { instrs, clip_h }
        } else {
            DrawOutcome::Inline(instrs)
        }
    }

    /// Clipboard contribution when selected: the rendered line.
    pub fn copy_text(&self, rec: &MsgRecord) -> Option<String> {
        let inner = self.inner.lock();
        inner.instances.get(&(rec.ts, rec.id)).map(|inst| inst.data.line_text())
    }

    /// Hit dispatch in message-local coordinates: URL rects first,
    /// then the nick prefix, then the expand affordance.
    pub fn hit_test(&self, rec: &MsgRecord, pos: Point) -> Option<Hit> {
        let inner = self.inner.lock();
        let Some(inst) = inner.instances.get(&(rec.ts, rec.id)) else { return None };
        for (url, rect) in &inst.url_rects {
            if rect.contains(pos) {
                return Some(Hit::Url(url.clone()))
            }
        }
        for (nick, rect) in &inst.nick_rects {
            if rect.contains(pos) {
                return Some(Hit::Nick(nick.clone()))
            }
        }
        if let Some(rect) = &inst.channel_rect {
            if rect.contains(pos) {
                return Some(Hit::Channel(inst.data.channel.clone()))
            }
        }
        if let Some(rect) = &inst.affordance_rect {
            if rect.contains(pos) {
                return Some(Hit::Expand)
            }
        }
        None
    }

    /// Toggle a capped message's expanded state and regen its rendered
    /// state; returns the new measured height.
    pub fn toggle_expand(&self, rec: &MsgRecord) -> f32 {
        let key = (rec.ts, rec.id);
        let mut expanded = self.expanded.lock();
        if !expanded.remove(&key) {
            expanded.insert(key);
        }
        drop(expanded);
        self.regen(&key);
        self.measure(rec)
    }

    /// The URL hit rects of a materialized instance (test access).
    pub(crate) fn url_rects(&self, rec: &MsgRecord) -> Vec<(String, Rectangle)> {
        let inner = self.inner.lock();
        inner
            .instances
            .get(&(rec.ts, rec.id))
            .map(|inst| inst.url_rects.clone())
            .unwrap_or_default()
    }

    /// The nick hit rects of a materialized instance (test access).
    pub(crate) fn nick_rects(&self, rec: &MsgRecord) -> Vec<(String, Rectangle)> {
        let inner = self.inner.lock();
        inner
            .instances
            .get(&(rec.ts, rec.id))
            .map(|inst| inst.nick_rects.clone())
            .unwrap_or_default()
    }

    /// The channel span's hit rect of a materialized instance, when
    /// rendered (test access).
    pub(crate) fn channel_rect(&self, rec: &MsgRecord) -> Option<Rectangle> {
        let inner = self.inner.lock();
        inner.instances.get(&(rec.ts, rec.id)).and_then(|inst| inst.channel_rect)
    }

    /// The channel chip rect of a materialized instance, when the
    /// span is rendered (test access).
    pub(crate) fn channel_chip(&self, rec: &MsgRecord) -> Option<Rectangle> {
        let inner = self.inner.lock();
        inner.instances.get(&(rec.ts, rec.id)).and_then(|inst| inst.channel_chip)
    }

    /// Build hit rects for the glyph runs whose brush matches
    /// `match_color`, mapping each run to its entry in `ranges` by
    /// intersecting the (coarse) font-run text range. Layout
    /// coordinates are physical, so divide by the scale to get the
    /// message-local virtual units hit tests use.
    fn compute_hit_rects(
        layout: &text::TextLayout,
        data: &PrivData,
        match_color: Color,
        ranges: &[std::ops::Range<usize>],
        payload_fn: impl Fn(&str) -> Option<String>,
    ) -> Vec<(String, Rectangle)> {
        let mut rects = vec![];
        if ranges.is_empty() {
            return rects
        }
        let linetext = data.line_text();
        let scale = layout.scale();
        for line in layout.lines() {
            for item in line.items() {
                let parley::PositionedLayoutItem::GlyphRun(glyph_run) = item else { continue };
                if glyph_run.style().brush != match_color {
                    continue
                }

                let font_range = glyph_run.run().text_range();
                let Some(hit_range) =
                    ranges.iter().find(|r| r.start < font_range.end && r.end > font_range.start)
                else {
                    continue
                };
                let Some(payload) = payload_fn(&linetext[hit_range.clone()]) else { continue };

                let metrics = glyph_run.run().metrics();
                let x = glyph_run.offset() / scale;
                let y = (glyph_run.baseline() - metrics.ascent) / scale;
                let w = glyph_run.advance() / scale;
                let h = (metrics.ascent + metrics.descent) / scale;
                rects.push((payload, Rectangle::new(x, y, w, h)));
            }
        }
        rects
    }

    /// Insert a confirmed privmsg into the buffer (display-only:
    /// persistence is the receive path's job — `handle_receive`
    /// persists through `persist_side_channel` before showing).
    /// Measure, insert, keep the view stable if the message grows
    /// content below the viewport. An insert for a message this view
    /// is already showing acts as its confirmation.
    pub async fn insert_line(
        &self,
        ts: Timestamp,
        id: MessageId,
        channel: String,
        nick: String,
        text: String,
    ) {
        self.insert_privmsg(ts, id, channel, nick, text, true).await
    }

    /// Insert an unconfirmed (sent, not yet seen on the network)
    /// privmsg — display-only, nothing is persisted; the trees only
    /// ever hold confirmed copies (written by the receive path when
    /// the network relay comes back).
    pub async fn insert_unconf_line(
        &self,
        ts: Timestamp,
        id: MessageId,
        channel: String,
        nick: String,
        text: String,
    ) {
        self.insert_privmsg(ts, id, channel, nick, text, false).await
    }

    async fn insert_privmsg(
        &self,
        ts: Timestamp,
        id: MessageId,
        channel: String,
        nick: String,
        text: String,
        confirmed: bool,
    ) {
        if ts <= 6047051717 {
            error!(target: "ui::chatview::privmsg", "rejecting insert with non-millisecond timestamp {ts}");
            return
        }
        t!("insert ts={ts} id={id} channel={channel} nick={nick} confirmed={confirmed}");

        // Display-only: persistence is the receive path's job
        // (`handle_receive` persists every message through
        // `persist_side_channel` before deciding to show it). The
        // buffer insert is the per-view gate: a composite key already
        // present means this view is showing it — the re-receive is
        // its confirmation (unconfirmed → confirmed flip), or a
        // duplicate relay, for which confirm is inert.
        let payload = codec::encode_privmsg_payload(&nick, &text, confirmed, &channel);
        let rec = MsgRecord { ts, id: id.clone(), msg_type: MsgType::PrivMsg, payload, height: 0. };
        let height = self.measure(&rec);

        let full_rec = MsgRecord { height, ..rec };
        let mut buffer = self.buffer.lock().await;
        if !buffer.insert(full_rec.clone()) {
            drop(buffer);
            self.confirm(id).await;
            return
        }
        let top = buffer.pos_of_key(&(full_rec.ts, full_rec.id));
        drop(buffer);

        // A fud URL in the text derives its file message below the line.
        if get_file_url(&text).is_some() {
            if let Some(chat) = self.chat.upgrade() {
                chat.derive_filemsg(&full_rec, &nick, &text).await;
            }
        }

        if let (Some(top), Some(chat)) = (top, self.chat.upgrade()) {
            // The appended message sits at the very bottom: when the
            // user reads history, shift by its height so nothing moves.
            let mut ctl = chat.controller.lock();
            let scroll = ctl.scroll();
            ctl.compensate(height, top <= scroll);
        }

        if let Some(chat) = self.chat.upgrade() {
            chat.sync_is_at_bottom();
            chat.redraw.trigger();
            chat.loader.wake(Wakeup::Insert);
        }
    }

    /// Mark an unconfirmed message confirmed: rewrite the payload in
    /// place (same kvdb entry), update the record, regen for styling.
    /// Mark an unconfirmed message confirmed: flip the buffered
    /// record's payload and regen for styling. Buffer-only — the trees
    /// only ever hold confirmed copies (persistence is the receive
    /// path's job).
    pub async fn confirm(&self, id: MessageId) {
        let (ts, payload) = {
            let buffer = self.buffer.lock().await;
            let Some(rec) = buffer.record(&id) else {
                t!("confirm of unloaded id={id}");
                return
            };
            (rec.ts, rec.payload.clone())
        };

        let (nick, text, confirmed, channel) = codec::decode_privmsg_payload(&payload, ts, &id);
        if confirmed {
            return
        }

        let new_payload = codec::encode_privmsg_payload(&nick, &text, true, &channel);
        {
            let mut buffer = self.buffer.lock().await;
            if let Some(rec) = buffer.record_mut(&id) {
                rec.payload = new_payload;
            }
        }
        self.regen(&(ts, id));

        if let Some(chat) = self.chat.upgrade() {
            chat.redraw.trigger();
        }
        t!("confirmed id={id}");
    }

    /// The scene node handle, for the chatview's method wiring.
    pub fn node(&self) -> &SceneNodeWeak {
        &self.node
    }

    /// The "Copied link" toast label.
    pub fn url_copy_text(&self) -> String {
        self.url_copy_text.get()
    }

    /// The toast's foreground color.
    pub fn url_copy_fg_color(&self) -> Color {
        self.url_copy_fg_color.get()
    }

    /// The toast's background color.
    pub fn url_copy_bg_color(&self) -> Color {
        self.url_copy_bg_color.get()
    }

    /// The toast label's font size.
    pub fn url_copy_font_size(&self) -> f32 {
        self.url_copy_font_size.get()
    }

    /// The toast's inner padding.
    pub fn url_copy_padding(&self) -> f32 {
        self.url_copy_padding.get()
    }

    /// The toast's lift above the anchor.
    pub fn url_copy_offset(&self) -> f32 {
        self.url_copy_offset.get()
    }

    /// How long the toast stays, in seconds.
    pub fn url_copy_duration(&self) -> f32 {
        self.url_copy_duration.get()
    }
}

static URL_REGEX: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();

fn url_regex() -> &'static regex::Regex {
    URL_REGEX
        .get_or_init(|| regex::Regex::new(r"https?://[^\s]+|fud://[^\s]+|www\.[^\s]+").unwrap())
}

/// The tight glyph bounds of a layout, in virtual units. Empty when
/// the layout holds no glyphs.
fn layout_glyph_bounds(layout: &text::TextLayout) -> Rectangle {
    let scale = layout.scale();
    let mut x0 = f32::MAX;
    let mut y0 = f32::MAX;
    let mut x1 = f32::MIN;
    let mut y1 = f32::MIN;
    for line in layout.lines() {
        for item in line.items() {
            let parley::PositionedLayoutItem::GlyphRun(run) = item else { continue };
            let metrics = run.run().metrics();
            let x = run.offset() / scale;
            let y = (run.baseline() - metrics.ascent) / scale;
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x + run.advance() / scale);
            y1 = y1.max(y + (metrics.ascent + metrics.descent) / scale);
        }
    }
    if x0 > x1 || y0 > y1 {
        return Rectangle::zero()
    }
    Rectangle::new(x0, y0, x1 - x0, y1 - y0)
}

/// A rectangle grown by `pad` on every side.
fn inflate_rect(rect: &Rectangle, pad: f32) -> Rectangle {
    Rectangle::new(rect.x - pad, rect.y - pad, rect.w + 2. * pad, rect.h + 2. * pad)
}

/// Truncate `s` until it fits `max_w` at `font_size`, marking the cut
/// with an ellipsis. Room names are short; this runs per
/// materialization only (cached in the instance). A monospace width
/// estimate bounds the probe start so an absurdly long name cannot
/// turn this into quadratic layout work.
fn ellipsize_to_width(s: &str, max_w: f32, font_size: f32, window_scale: f32) -> String {
    let chars: Vec<char> = s.chars().collect();
    let estimate = ((max_w / (font_size * 0.6)).ceil() as usize).saturating_add(4);
    let mut len = chars.len().min(estimate);
    while len > 0 {
        let mut candidate: String = chars[..len].iter().collect();
        if len < chars.len() {
            candidate.push('…');
        }
        let probe = text::make_layout(
            &candidate,
            [1., 1., 1., 1.],
            font_size,
            1.2,
            window_scale,
            None,
            &[],
        );
        if probe.width() <= max_w {
            return candidate
        }
        len -= 1;
    }
    s.chars().take(1).collect()
}

/// Sanitize an extracted URL match for use as a hit payload (opening,
/// copying): strip trailing punctuation and control characters (an
/// interior NUL would abort the Android intent JNI path), resolve
/// schemeless `www.` hosts to https, and reject anything that does not
/// parse to an http/https/fud URL. Returns None for matches that are
/// not safely openable.
fn sanitize_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim_end_matches(['.', ',', '!', '?', ')', ']', '}', '\'', '"', ';', ':']);
    let trimmed = trimmed.trim_end_matches(|c: char| c.is_control());
    if trimmed.chars().any(|c| c.is_control()) {
        return None
    }
    let candidate = if trimmed.starts_with("www.") {
        format!("https://{trimmed}")
    } else {
        trimmed.to_string()
    };
    let url = Url::parse(&candidate).ok()?;
    match url.scheme() {
        "http" | "https" | "fud" => Some(url.to_string()),
        _ => None,
    }
}

/// URL byte ranges (no colors) within the rendered line text.
fn url_ranges_of(data: &PrivData, color: Color) -> Vec<std::ops::Range<usize>> {
    let mut ranges = vec![];
    for (range, _) in url_color_ranges(&data.text, data.body_offset(), color) {
        ranges.push(range);
    }
    ranges
}

/// URL byte ranges within the rendered line text, colored with the URL
/// color so backgrounds and hit rects can find them by brush.
fn url_color_ranges(
    text: &str,
    offset: usize,
    color: Color,
) -> Vec<(std::ops::Range<usize>, Color)> {
    let mut ranges = vec![];
    for m in url_regex().find_iter(text) {
        ranges.push((m.start() + offset..m.end() + offset, color));
    }
    ranges
}

/// Scene node factory for the privmsg type node.

/// Decode `(ts, id, channel, nick, text)` method-call data.
pub fn decode_insert_data(data: &[u8]) -> Option<(Timestamp, MessageId, String, String, String)> {
    let mut cur = Cursor::new(data);
    let ts = Timestamp::decode(&mut cur).ok()?;
    let id = MessageId::decode(&mut cur).ok()?;
    let channel = String::decode(&mut cur).ok()?;
    let nick = String::decode(&mut cur).ok()?;
    let text = String::decode(&mut cur).ok()?;
    Some((ts, id, channel, nick, text))
}

/// Encode `(ts, id, channel, nick, text)` for method-call data.
pub fn encode_insert_data(
    ts: Timestamp,
    id: &MessageId,
    channel: &str,
    nick: &str,
    text: &str,
) -> Vec<u8> {
    let mut data = vec![];
    ts.encode(&mut data).unwrap();
    id.encode(&mut data).unwrap();
    channel.encode(&mut data).unwrap();
    nick.encode(&mut data).unwrap();
    text.encode(&mut data).unwrap();
    data
}

#[async_trait]
impl UIObject for PrivMsgNode {
    fn priority(&self) -> u32 {
        0
    }
}

impl std::fmt::Debug for PrivMsgNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.node.upgrade())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{app::node::create_privmsg_node, prop::PropertyAtomicGuard};
    use std::collections::HashSet;

    /// A scale=1.0 property standing in for the window scale.
    fn scale_prop() -> PropertyFloat32 {
        let mut node = SceneNode::new("w", SceneNodeType::Object);
        let prop = Property::new(
            "scale",
            PropertyType::Float32,
            PropertySubType::Null,
            PropertyPermission::default(),
        );
        node.add_property(prop).unwrap();
        let node = node.setup_null();
        let atom = &mut PropertyAtomicGuard::none();
        node.set_property_f32(atom, Role::App, "scale", 1.).unwrap();
        PropertyFloat32::wrap(&node, Role::Internal, "scale", 0).unwrap()
    }

    /// A loader over a throwaway tree, for insert-path tests.
    fn fixture_loader(tag: &str) -> (Arc<Loader>, kvdb_overlay::Tree) {
        let path = std::env::temp_dir()
            .join(format!("darkfi-chatview-privmsg-{tag}-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let db = kvdb_overlay::Database::open_default(&path).unwrap();
        let tree = db.open_tree_default("#dev__chat_tree_v3").unwrap();
        (
            Loader::new(
                Arc::new(AsyncMutex::new(MsgBuffer::new())),
                crate::ui::RedrawTrigger::new().0,
            ),
            tree.clone(),
        )
    }

    async fn make_node(tag: &str) -> (SceneNodePtr, PrivMsgNodePtr, Arc<AsyncMutex<MsgBuffer>>) {
        // A chatview-shaped parent supplies the shared styling; the
        // privmsg node carries the type-specific properties.
        let chat = crate::app::node::create_chatview("chatview");
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

        let shared = SharedProps::wrap(&chat, scale_prop());

        let mut raw = MsgBuffer::new();
        raw.disable_separators();
        let buffer = Arc::new(AsyncMutex::new(raw));
        let (redraw, _rx) = crate::ui::RedrawTrigger::new();
        let loader = Loader::new(buffer.clone(), redraw);
        loader.bind("#dev".to_string(), fixture_loader(tag).1);

        // A dangling chat weak: insert still displays and buffers, it
        // just skips view compensation (no chatview to talk to).
        let chat_weak: Weak<ChatView> = Weak::new();

        let node = create_privmsg_node("privmsg");
        let atom = &mut PropertyAtomicGuard::none();
        let prop = node.get_property("nick_colors").unwrap();
        for c in [1., 0., 0., 1.] {
            prop.push_f32(atom, Role::App, c).unwrap();
        }
        let prop = node.get_property("action_text_color").unwrap();
        for (i, c) in [0.5, 0.25, 0.75, 1.].iter().enumerate() {
            prop.set_f32(atom, Role::App, i, *c).unwrap();
        }
        let shared2 = shared.clone();
        let loader2 = loader.clone();
        let buffer2 = buffer.clone();
        let node =
            node.setup(|me| async move {
                PrivMsgNode::new(me, shared2, loader2, buffer2, chat_weak).await
            })
            .await;
        chat.link(node.clone());
        let Pimpl::PrivMsgNode(ptr) = node.pimpl() else { panic!() };
        (chat, ptr.clone(), buffer)
    }

    fn rec_of(ts: Timestamp, idb: u8, text: &str, confirmed: bool) -> MsgRecord {
        let mut id = [0u8; 32];
        id[0] = idb;
        let payload = codec::encode_privmsg_payload("alice", text, confirmed, "#dev");
        MsgRecord { ts, id: MessageId(id), msg_type: MsgType::PrivMsg, payload, height: 0. }
    }

    #[test]
    fn measure_reports_height_and_caches_layout() {
        let (_chat, node, _buffer) = smol::block_on(make_node("cache"));

        let rec = rec_of(1_000_000, b'a', "hello world", true);
        let h1 = node.measure(&rec);
        assert!(h1 > 4., "height incl. spacing: {h1}");
        assert_eq!(node.inner.lock().layout_builds, 1);

        // Repeated measures reuse the cached layout.
        let h2 = node.measure(&rec);
        assert_eq!(h1, h2);
        assert_eq!(node.inner.lock().layout_builds, 1);
        assert!(node.is_materialized(&rec));
    }

    #[test]
    fn release_then_materialize_rebuilds_state() {
        let (_chat, node, _buffer) = smol::block_on(make_node("release"));

        let rec = rec_of(1_000_000, b'a', "hello", true);
        let h1 = node.measure(&rec);
        node.release(&(rec.ts, rec.id));
        assert!(!node.is_materialized(&rec));

        let h2 = node.measure(&rec);
        assert_eq!(h1, h2);
        assert_eq!(node.inner.lock().layout_builds, 2);
    }

    #[test]
    fn width_change_rewraps() {
        let (chat, node, _buffer) = smol::block_on(make_node("width"));
        let long = "word ".repeat(60);
        let rec = rec_of(1_000_000, b'a', &long, true);

        let wide = node.measure(&rec);
        assert_eq!(node.inner.lock().layout_builds, 1);

        // Narrow the chatview rect: the signature changes, the layout
        // re-wraps taller.
        let prop = chat.get_property("rect").unwrap();
        let atom = &mut PropertyAtomicGuard::none();
        prop.set_f32(atom, Role::App, 2, 200.).unwrap();

        let narrow = node.measure(&rec);
        assert!(narrow > wide, "narrow={narrow} wide={wide}");
        assert_eq!(node.inner.lock().layout_builds, 2);
    }

    #[test]
    fn message_variants_indent_only_first_line() {
        let (chat, node, _buffer) = smol::block_on(make_node("leftwrap"));
        let atom = &mut PropertyAtomicGuard::none();
        chat.get_property("rect").unwrap().set_f32(atom, Role::App, 2, 240.).unwrap();
        // Fixed spacing so the expected first-line offset is exact:
        // timestamp glyph width + spacing.
        node.node()
            .upgrade()
            .unwrap()
            .set_property_f32(atom, Role::App, "channel_spacing", 20.)
            .unwrap();
        let body = "word ".repeat(30);
        for (nick, text, copied) in [
            ("alice", body.clone(), format!("alice {body}")),
            ("alice", format!("\u{1}ACTION {body}\u{1}"), format!("* alice {body}")),
            ("NOTICE", body.clone(), body.clone()),
        ] {
            let mut rec = rec_of(1_000_000, b'a', &text, true);
            rec.payload = codec::encode_privmsg_payload(nick, &text, true, "#dev");
            let height = node.measure(&rec);
            assert_eq!(node.copy_text(&rec), Some(copied));
            let inner = node.inner.lock();
            let inst = &inner.instances[&(rec.ts, rec.id)];
            assert!(inst.txt_layout.lines().count() > 2);
            let expected_first = inst.ts_layout.width() + 20.;
            for (i, line) in inst.txt_layout.lines().enumerate() {
                // The indent lives in the line's inline_min_coord (the
                // breaker positions the line at the indent; align does
                // not carry it in metrics().offset).
                let expected = if i == 0 { expected_first } else { 0. };
                assert!(
                    (line.metrics().inline_min_coord - expected).abs() < 0.1,
                    "line {i} starts at {expected}"
                );
                assert_eq!(line.metrics().inline_max_coord, 240.);
            }
            assert!((height - inst.txt_layout.height() - 4.).abs() < 0.01);
        }
    }

    #[test]
    fn channel_spacing_change_rebuilds_first_line() {
        let (_chat, node, _buffer) = smol::block_on(make_node("chanspacing"));
        let rec = rec_of(1_000_000, b'a', "hello", true);
        let height = node.measure(&rec);
        let ts_w = {
            let inner = node.inner.lock();
            inner.instances[&(rec.ts, rec.id)].ts_layout.width()
        };

        let atom = &mut PropertyAtomicGuard::none();
        node.node()
            .upgrade()
            .unwrap()
            .set_property_f32(atom, Role::App, "channel_spacing", 30.)
            .unwrap();
        assert_eq!(node.measure(&rec), height);
        assert_eq!(node.inner.lock().layout_builds, 2);
        assert!((node.nick_rects(&rec)[0].1.x - (ts_w + 30.)).abs() < 0.5);

        node.shared.window_scale.set(atom, 1.5);
        assert!((node.measure(&rec) - height).abs() < 0.01);
        assert_eq!(node.inner.lock().layout_builds, 3);
        assert!((node.nick_rects(&rec)[0].1.x - (ts_w + 30.)).abs() < 0.5);
    }

    #[test]
    fn styling_and_data_changes_invalidate() {
        let (chat, node, _buffer) = smol::block_on(make_node("styling"));
        let rec = rec_of(1_000_000, b'a', "hello", true);
        node.measure(&rec);
        assert_eq!(node.inner.lock().layout_builds, 1);

        // Styling change (font size on the chatview node).
        let atom = &mut PropertyAtomicGuard::none();
        chat.set_property_f32(atom, Role::App, "font_size", 20.).unwrap();
        node.measure(&rec);
        assert_eq!(node.inner.lock().layout_builds, 2, "font size change rebuilds");

        // Data change: confirm flips the body color (part of the signature).
        let rec_unconf = rec_of(1_000_000, b'a', "hello", false);
        node.measure(&rec_unconf);
        assert_eq!(node.inner.lock().layout_builds, 3, "confirmed flag rebuilds");
    }

    #[test]
    fn insert_line_displays_and_buffers() {
        let (_chat, node, buffer) = smol::block_on(make_node("insert"));
        smol::block_on(async {
            node.insert_line(
                1_756_000_000_000,
                MessageId([b'x'; 32]),
                "#dev".to_string(),
                "alice".to_string(),
                "hi there".to_string(),
            )
            .await;
        });

        let buffer = smol::block_on(buffer.lock());
        assert_eq!(buffer.len(), 1);
        let rec = buffer.record(&MessageId([b'x'; 32])).unwrap();
        assert!(rec.height > 0., "inserted with measured height");
        let (_, _, confirmed, channel) =
            codec::decode_privmsg_payload(&rec.payload, rec.ts, &rec.id);
        assert!(confirmed);
        assert_eq!(channel, "#dev", "payload carries the channel");
    }

    #[test]
    fn confirm_flips_buffer_in_place() {
        let (_chat, node, buffer) = smol::block_on(make_node("confirm"));
        let id = MessageId([b'x'; 32]);
        let ts = 1_756_000_000_000;

        smol::block_on(async {
            node.insert_unconf_line(
                ts,
                id,
                "#dev".to_string(),
                "alice".to_string(),
                "hi".to_string(),
            )
            .await;
            node.confirm(id).await;
        });

        let buffer = smol::block_on(buffer.lock());
        assert_eq!(buffer.len(), 1, "no duplicate record");
        let rec = buffer.record(&id).unwrap();
        let (_, _, confirmed, _) = codec::decode_privmsg_payload(&rec.payload, rec.ts, &rec.id);
        assert!(confirmed, "confirmed in place in the buffer");
    }

    #[test]
    fn insert_dedups_and_confirms_in_place() {
        let (_chat, node, buffer) = smol::block_on(make_node("dedup"));
        let id = MessageId([b'x'; 32]);
        let ts = 1_756_000_000_000;

        smol::block_on(async {
            node.insert_unconf_line(
                ts,
                id,
                "#dev".to_string(),
                "alice".to_string(),
                "hi".to_string(),
            )
            .await;
        });
        {
            let buffer = smol::block_on(buffer.lock());
            assert_eq!(buffer.len(), 1);
            let rec = buffer.record(&id).unwrap();
            let (_, _, confirmed, _) = codec::decode_privmsg_payload(&rec.payload, rec.ts, &rec.id);
            assert!(!confirmed, "starts unconfirmed");
        }

        // The confirmed relay of the same message updates in place:
        // one entry, confirmed payload, regen invalidated the rendered
        // state (lazily rebuilt on the next touch).
        smol::block_on(async {
            node.insert_line(ts, id, "#dev".to_string(), "alice".to_string(), "hi".to_string())
                .await;
        });
        {
            let buffer = smol::block_on(buffer.lock());
            assert_eq!(buffer.len(), 1, "no duplicate record");
            let rec = buffer.record(&id).unwrap();
            let (_, _, confirmed, _) = codec::decode_privmsg_payload(&rec.payload, rec.ts, &rec.id);
            assert!(confirmed, "confirmed in place");
        }
        assert!(
            !{
                let buffer = smol::block_on(buffer.lock());
                let rec = buffer.record(&id).unwrap().clone();
                node.is_materialized(&rec)
            },
            "regen dropped the rendered state"
        );
        let builds_before = node.inner.lock().layout_builds;
        let rec = {
            let buffer = smol::block_on(buffer.lock());
            buffer.record(&id).unwrap().clone()
        };
        node.measure(&rec);
        assert_eq!(
            node.inner.lock().layout_builds,
            builds_before + 1,
            "rebuilt with confirmed data"
        );

        // A plain duplicate changes nothing.
        let builds = node.inner.lock().layout_builds;
        smol::block_on(async {
            node.insert_line(ts, id, "#dev".to_string(), "alice".to_string(), "hi".to_string())
                .await;
        });
        let buffer = smol::block_on(buffer.lock());
        assert_eq!(buffer.len(), 1);
        drop(buffer);
        assert_eq!(node.inner.lock().layout_builds, builds, "already-confirmed duplicate is inert");
    }

    #[test]
    fn line_text_variants() {
        let data = PrivData {
            ts: 0,
            id: MessageId([0; 32]),
            nick: "alice".to_string(),
            text: "waves".to_string(),
            confirmed: true,
            channel: "#dev".to_string(),
            is_action: true,
            is_notice: false,
            expanded: true,
        };
        assert_eq!(data.line_text(), "* alice waves");
        assert_eq!(data.body_offset(), "alice".len() + 3);

        let data = PrivData { is_action: false, ..data };
        assert_eq!(data.line_text(), "alice waves");
        assert_eq!(data.body_offset(), "alice".len() + 1);

        let data = PrivData { nick: "NOTICE".to_string(), is_notice: true, ..data };
        assert_eq!(data.line_text(), "waves");
        assert_eq!(data.body_offset(), 0);
    }

    #[test]
    fn channel_span_renders_only_when_shown() {
        let (chat, node, _buffer) = smol::block_on(make_node("chanspan"));

        let rec = rec_of(1_000_000, b'a', "hello", true);
        node.measure(&rec);
        assert!(node.channel_rect(&rec).is_none(), "hidden by default");

        // No channel hit in the gutter while display is inactive.
        assert_eq!(node.hit_test(&rec, Point::new(5., 25.)), None);

        // Toggling display rebuilds the instance with the span.
        let atom = &mut PropertyAtomicGuard::none();
        chat.set_property_bool(atom, Role::App, "show_channel", true).unwrap();
        let builds = node.inner.lock().layout_builds;
        node.measure(&rec);
        assert_eq!(node.inner.lock().layout_builds, builds + 1, "toggle regenerates");

        let rect = node.channel_rect(&rec).expect("span rendered");
        assert!(rect.x > 0., "after the timestamp: {:?}", rect);
        let mid = Point::new(rect.x + rect.w / 2., rect.y + rect.h / 2.);
        assert_eq!(node.hit_test(&rec, mid), Some(Hit::Channel("#dev".to_string())));

        // The chip hugs the label — it must not reach back over the
        // timestamp (x=0) and must cover the label's middle.
        let chip = node.channel_chip(&rec).expect("chip rect present");
        assert!(chip.x > 0., "chip clear of the timestamp: {:?}", chip);
        assert!(chip.x <= mid.x && mid.x <= chip.x + chip.w, "chip covers the label");
        assert!(chip.y <= mid.y && mid.y <= chip.y + chip.h, "chip covers the label vertically");

        // The spacing property widens the timestamp → chip gap (and
        // the chip → line indent) and regens the rendered state.
        let chip_x = chip.x;
        node.node()
            .upgrade()
            .unwrap()
            .set_property_f32(atom, Role::App, "channel_spacing", 30.)
            .unwrap();
        node.measure(&rec);
        let chip2 = node.channel_chip(&rec).expect("chip after respacing");
        assert!(chip2.x > chip_x + 20., "gap widened: {chip_x:?} -> {:?}", chip2.x);

        // Toggling back off removes the span and its hit target.
        chat.set_property_bool(atom, Role::App, "show_channel", false).unwrap();
        node.measure(&rec);
        assert!(node.channel_rect(&rec).is_none(), "hidden again");
        assert_eq!(node.hit_test(&rec, mid), None);
    }

    #[test]
    fn channel_ellipsizes_when_absurdly_long() {
        use super::ellipsize_to_width;
        assert_eq!(ellipsize_to_width("#dev", 240., 10., 1.), "#dev");
        let long = ellipsize_to_width("#a-very-long-channel-name-here", 50., 10., 1.);
        assert!(long.ends_with('…'), "cut marked: {long}");
        assert!(long.chars().count() < "#a-very-long-channel-name-here".chars().count());
        // The ellipsized form fits the bound.
        let probe = text::make_layout(&long, [1., 1., 1., 1.], 10., 1.2, 1., None, &[]);
        assert!(probe.width() <= 50.);
    }

    #[test]
    fn wrapped_lines_return_to_the_left_edge() {
        let (chat, node, _buffer) = smol::block_on(make_node("outdent"));

        // A message long enough to wrap at the full width.
        let long = "word ".repeat(60);
        let rec = rec_of(1_000_000, b'a', &long, true);
        node.measure(&rec);

        // The first line is indented past the gutter; the second line
        // starts at the view's left edge (x=0).
        let layout = {
            let inner = node.inner.lock();
            inner.instances.get(&(rec.ts, rec.id)).expect("materialized").txt_layout.clone()
        };
        let mut lines = layout.lines();
        let scale = layout.scale();
        let line_x = |line: parley::Line<'_, Color>| {
            let mut x = None;
            for item in line.items() {
                if let parley::PositionedLayoutItem::GlyphRun(run) = item {
                    x = Some(run.offset());
                    break
                }
            }
            x.unwrap_or(0.) / scale
        };
        let first = lines.next().expect("first line");
        let second = lines.next().expect("wrapped second line");
        let first_x = line_x(first);
        let second_x = line_x(second);
        // The first line starts after the timestamp glyphs plus the
        // spacing (auto here: a fraction of the channel font size).
        // The exact ts width is font-metric dependent ("HH:MM" at
        // font 10 ≈ 30px), so assert a conservative lower bound.
        assert!(first_x >= 25., "first line clears the timestamp: {first_x}");
        assert!(second_x.abs() < 0.5, "continuation at the left edge: {second_x}");

        // The wrap width is the full view width, not width minus gutter.
        assert!(layout.width() <= 800.);

        let _ = chat;
    }

    #[test]
    fn url_hit_rects_resolve_through_hit_test() {
        let (_chat, node, _buffer) = smol::block_on(make_node("urls"));

        let rec = rec_of(1_000_000, b'a', "see https://example.com/now okay", true);
        node.measure(&rec);

        let rects = node.url_rects(&rec);
        assert_eq!(rects.len(), 1, "one URL run");
        let (url, rect) = &rects[0];
        assert_eq!(url, "https://example.com/now");

        // Inside the rect: the URL; on the nick prefix: the nick;
        // elsewhere: nothing.
        let mid = Point::new(rect.x + rect.w / 2., rect.y + rect.h / 2.);
        assert_eq!(node.hit_test(&rec, mid), Some(Hit::Url(url.clone())));

        let nicks = node.nick_rects(&rec);
        assert!(!nicks.is_empty(), "nick prefix is clickable");
        let (nick, nrect) = &nicks[0];
        assert_eq!(nick, "alice");
        // The nick starts at the first-line indent: timestamp glyph
        // width plus the channel spacing (the factory default here —
        // the test harness doesn't set it).
        let ts_w = {
            let inner = node.inner.lock();
            inner.instances[&(rec.ts, rec.id)].ts_layout.width()
        };
        assert!((nrect.x - (ts_w + 4.)).abs() < 0.5, "nick at the indent: {}", nrect.x);
        assert_eq!(
            node.hit_test(&rec, Point::new(nrect.x + nrect.w / 2., nrect.y + nrect.h / 2.)),
            Some(Hit::Nick("alice".to_string()))
        );

        assert_eq!(node.hit_test(&rec, Point::new(400., rect.y + rect.h / 2.)), None);
    }

    #[test]
    fn wrapped_url_produces_hit_rects() {
        let (_chat, node, _buffer) = smol::block_on(make_node("wrapurls"));

        // A long URL prefix that must wrap across lines still yields
        // hit rects (one per wrapped run).
        let mut text = String::from("look ");
        for _ in 0..30 {
            text.push_str("https://example.com/very/long/path/segment ");
        }
        let rec = rec_of(1_000_000, b'a', &text, true);
        node.measure(&rec);

        let rects = node.url_rects(&rec);
        assert!(rects.len() >= 2, "wrapped URL runs: {}", rects.len());
        for (url, rect) in &rects {
            assert!(url.starts_with("https://example.com/"), "{url}");
            assert_eq!(
                node.hit_test(&rec, Point::new(rect.x + 1., rect.y + rect.h / 2.)),
                Some(Hit::Url(url.clone()))
            );
        }
        assert!(rects.iter().any(|(_, rect)| rect.x == 0. && rect.y > 20.));
    }

    #[test]
    fn sweep_releases_out_of_window_and_rematerialize_rebuilds() {
        let (_chat, node, _buffer) = smol::block_on(make_node("sweep"));

        // Materialize a window of 5 records plus older strays.
        let mut recs = vec![];
        for i in 0..8u64 {
            recs.push(rec_of(1_000_000 + i * 60_000, i as u8 + b'a', "text", true));
        }
        for rec in &recs {
            node.measure(rec);
        }
        assert_eq!(node.instance_count(), 8);

        // The window keeps the newest 5; budget 0 releases the rest.
        let mut keep = HashSet::new();
        for rec in &recs[3..] {
            keep.insert((rec.ts, rec.id));
        }
        node.sweep(&keep, 0);
        assert_eq!(node.instance_count(), 5, "window members survive");
        for rec in &recs[..3] {
            assert!(!node.is_materialized(rec), "stray released");
        }

        // Rematerializing a released record rebuilds identical state.
        let h1 = node.measure(&recs[0]);
        assert!(h1 > 0.);
        assert!(node.is_materialized(&recs[0]));
    }

    #[test]
    fn capped_measurement_and_expand_height_reporting() {
        let (chat, node, _buffer) = smol::block_on(make_node("cap"));

        // Cap the node at 60 px; a very long wrapped message towers
        // over it. Shorten the width so even modest text wraps tall.
        let atom = &mut PropertyAtomicGuard::none();
        node.node()
            .upgrade()
            .unwrap()
            .set_property_f32(atom, Role::App, "cap_max_height", 60.)
            .unwrap();
        let prop = chat.get_property("rect").unwrap();
        prop.set_f32(atom, Role::App, 2, 240.).unwrap();

        let long = "wrap me please ".repeat(80);
        let rec = rec_of(1_000_000, b'a', &long, true);
        let collapsed_h = node.measure(&rec);
        assert!((collapsed_h - 60. - 4.).abs() < 0.5, "collapsed to cap + spacing: {collapsed_h}");

        // The affordance is hit-testable in message-local coordinates.
        let affordance = {
            let inner = node.inner.lock();
            let inst = inner.instances.get(&(rec.ts, rec.id)).unwrap();
            inst.affordance_rect.clone().unwrap()
        };
        assert_eq!(
            node.hit_test(&rec, Point::new(affordance.x + 5., affordance.y + 5.)),
            Some(Hit::Expand)
        );

        // Expanding reports the full wrapped height.
        let expanded_h = node.toggle_expand(&rec);
        assert!(expanded_h > 200., "expanded height: {expanded_h}");
        assert_eq!(node.measure(&rec), expanded_h, "stable while expanded");

        // Collapsing again restores the cap.
        let again = node.toggle_expand(&rec);
        assert!((again - collapsed_h).abs() < 0.5, "re-collapsed: {again}");
    }

    #[test]
    fn short_messages_are_never_capped() {
        let (chat, node, _buffer) = smol::block_on(make_node("nocap"));
        let atom = &mut PropertyAtomicGuard::none();
        node.node()
            .upgrade()
            .unwrap()
            .set_property_f32(atom, Role::App, "cap_max_height", 60.)
            .unwrap();

        let rec = rec_of(1_000_000, b'a', "tiny", true);
        let h = node.measure(&rec);
        assert!(h < 60., "single short line: {h}");
        let inner = node.inner.lock();
        let inst = inner.instances.get(&(rec.ts, rec.id)).unwrap();
        assert!(inst.affordance_rect.is_none());
    }

    #[test]
    fn url_sanitization() {
        use super::sanitize_url;
        assert_eq!(sanitize_url("https://example.com/").as_deref(), Some("https://example.com/"));
        assert_eq!(
            sanitize_url("https://example.com/path.").as_deref(),
            Some("https://example.com/path")
        );
        assert_eq!(
            sanitize_url("https://example.com/a,b!").as_deref(),
            Some("https://example.com/a,b")
        );
        assert_eq!(sanitize_url("www.example.com/x").as_deref(), Some("https://www.example.com/x"));
        // Interior control characters (incl. NUL): rejected. A trailing
        // one is trimmed — the cleaned URL stays usable.
        assert_eq!(sanitize_url("https://evil.com/\u{0}x"), None);
        assert_eq!(sanitize_url("https://evil.com/x\u{0}").as_deref(), Some("https://evil.com/x"));
        // Non-http(s)/fud schemes cannot appear via the regex, but the
        // parser gate holds anyway.
        assert_eq!(sanitize_url("file:///etc/passwd"), None);
        // fud URLs stay untouched.
        assert_eq!(
            sanitize_url("fud://abcdef012345/file.png").as_deref(),
            Some("fud://abcdef012345/file.png")
        );
    }

    #[test]
    fn ctcp_action_parsing() {
        assert_eq!(parse_ctcp_action("\u{1}ACTION waves\u{1}"), Some("waves"));
        assert_eq!(parse_ctcp_action("\u{1}ACTION waves"), Some("waves"));
        assert_eq!(parse_ctcp_action("waves"), None);
        assert_eq!(parse_ctcp_action("\u{1}PING\u{1}"), None);
    }
}
