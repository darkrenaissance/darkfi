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

//! The date-separator type node. Separators are derived records owned
//! by the buffer — synthetic `(local-midnight, zero id)` keys, never
//! persisted — materialized through this node like any other message
//! type. No methods, no signals; copy text is the date label.

use async_trait::async_trait;
use chrono::{Local, TimeZone};
use parking_lot::Mutex as SyncMutex;
use std::{collections::HashMap, sync::Arc};

use crate::{
    gfx::{DrawInstruction, Point, Renderer},
    mesh::Color,
    prop::{Property, PropertyColor, PropertyFloat32, PropertySubType, PropertyType, Role},
    scene::{CallArgType, Pimpl, SceneNode, SceneNodeType, SceneNodeWeak},
    text,
    ui::UIObject,
};

use super::{privmsg::InstKey, Hit, SharedProps};
use crate::ui::chatview::{codec, MessageId, MsgRecord, Timestamp};

macro_rules! t { ($($arg:tt)*) => { trace!(target: "ui::chatview::datemsg", $($arg)*); } }

/// The rendered label for a separator day: "Sat 29 Aug 2026".
pub fn datestr(midnight: Timestamp) -> String {
    let Some(dt) = Local.timestamp_millis_opt(midnight as i64).single() else {
        return String::new()
    };
    dt.format("%a %-d %b %Y").to_string()
}

/// One materialized separator instance.
pub struct DateMsg {
    label: String,
    sig: DateSig,
    layout: text::TextLayout,
    instrs: Option<Vec<DrawInstruction>>,
    height: f32,
}

#[derive(PartialEq)]
struct DateSig {
    font_size: f32,
    line_height: f32,
    window_scale: f32,
    color: Color,
}

struct DateInner {
    instances: HashMap<InstKey, DateMsg>,
    /// Last-access counter per instance, for LRU eviction.
    touches: HashMap<InstKey, u64>,
    /// Monotonic access counter driving `touches`.
    access: u64,
    layout_builds: usize,
}

pub type DateMsgNodePtr = Arc<DateMsgNode>;

/// The date-separator type node.
pub struct DateMsgNode {
    node: SceneNodeWeak,
    shared: SharedProps,
    /// Type-local font size (null = inherit the chatview's).
    font_size: crate::prop::PropertyPtr,
    color: PropertyColor,
    inner: SyncMutex<DateInner>,
}

impl DateMsgNode {
    pub async fn new(node: SceneNodeWeak, shared: SharedProps) -> Pimpl {
        let node_ref = &node.upgrade().unwrap();
        let color = PropertyColor::wrap(node_ref, Role::Internal, "color").expect("datemsg color");
        let font_size = node_ref.get_property("font_size").expect("datemsg font_size");

        let self_ = Arc::new(Self {
            node: node.clone(),
            shared,
            font_size,
            color,
            inner: SyncMutex::new(DateInner {
                instances: HashMap::new(),
                touches: HashMap::new(),
                access: 0,
                layout_builds: 0,
            }),
        });
        Pimpl::DateMsgNode(self_)
    }

    /// The effective font size: the node's own when set, else the
    /// chatview's (null default).
    fn effective_font_size(&self) -> f32 {
        match self.font_size.get_f32_opt(0) {
            Ok(Some(v)) if v > 0. => v,
            _ => self.shared.font_size.get(),
        }
    }

    fn current_sig(&self) -> DateSig {
        DateSig {
            font_size: self.effective_font_size(),
            line_height: self.shared.line_height.get(),
            window_scale: self.shared.window_scale.get(),
            color: self.color.get(),
        }
    }

    /// The instance, (re)materialized as needed; returns its height.
    fn ensure_materialized(&self, rec: &MsgRecord) -> f32 {
        let key = (rec.ts, rec.id);
        let sig = self.current_sig();
        let mut inner = self.inner.lock();
        inner.access += 1;
        let access = inner.access;
        inner.touches.insert(key, access);
        if let Some(inst) = inner.instances.get(&key) {
            if inst.sig == sig {
                return inst.height
            }
        }

        let midnight = codec::decode_datemsg_payload(&rec.payload, rec.ts, &rec.id);
        let label = datestr(midnight);
        // The line box scales with the effective font size, keeping the
        // chatview's line-height ratio; a smaller date font gives a
        // proportionally smaller row.
        let font_size = self.effective_font_size();
        let line_ratio = self.shared.line_height.get() / self.shared.font_size.get();
        let line_height = font_size * line_ratio;
        let layout = text::make_layout(
            &label,
            sig.color,
            font_size,
            line_ratio,
            sig.window_scale,
            None,
            &[],
        );
        let height = line_height + self.shared.message_spacing.get();
        let inst = DateMsg { label, sig, layout, instrs: None, height };
        let height = inst.height;
        inner.instances.insert(key, inst);
        inner.layout_builds += 1;
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

    /// Drop an instance's rendered state.
    pub fn release(&self, key: &InstKey) {
        let mut inner = self.inner.lock();
        inner.instances.remove(key);
        inner.touches.remove(key);
    }

    /// Drop every instance's rendered state.
    pub fn release_all(&self) {
        let mut inner = self.inner.lock();
        inner.instances.clear();
        inner.touches.clear();
    }

    /// Rebuild rendered state from live props + current data.
    pub fn regen(&self, key: &InstKey) {
        self.release(key);
    }

    /// Rebuild every instance's rendered state.
    pub fn regen_all(&self) {
        self.release_all();
    }

    /// Release the out-of-window instances beyond the LRU budget.
    pub fn sweep(&self, keep: &std::collections::HashSet<InstKey>, budget: usize) {
        let releases = {
            let inner = self.inner.lock();
            super::evict_beyond(keep, &inner.touches, budget)
        };
        for key in releases {
            self.release(&key);
        }
    }

    /// Renderer-bound draw instructions in message-local coordinates.
    pub fn draw(&self, rec: &MsgRecord, renderer: &Renderer) -> Vec<DrawInstruction> {
        let key = (rec.ts, rec.id);
        let mut inner = self.inner.lock();
        inner.access += 1;
        let access = inner.access;
        inner.touches.insert(key, access);
        let Some(inst) = inner.instances.get_mut(&key) else { return vec![] };
        if inst.instrs.is_none() {
            let instrs = text::render_layout(
                &inst.layout,
                renderer,
                crate::gfx::gfxtag!("chatview_datemsg"),
            );
            inst.instrs = Some(instrs);
        }
        inst.instrs.clone().unwrap_or_default()
    }

    /// Clipboard contribution when selected: the date label.
    pub fn copy_text(&self, rec: &MsgRecord) -> Option<String> {
        let inner = self.inner.lock();
        inner.instances.get(&(rec.ts, rec.id)).map(|inst| inst.label.clone())
    }

    /// Separators carry no interactive content.
    pub fn hit_test(&self, _rec: &MsgRecord, _pos: Point) -> Option<Hit> {
        None
    }

    /// The scene node handle.
    pub fn node(&self) -> &SceneNodeWeak {
        &self.node
    }
}

/// Scene node factory for the date-separator type node.

#[async_trait]
impl UIObject for DateMsgNode {
    fn priority(&self) -> u32 {
        0
    }
}

impl std::fmt::Debug for DateMsgNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.node.upgrade())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::node::create_datemsg_node,
        prop::PropertyAtomicGuard,
        scene::SceneNodePtr,
        ui::chatview::{buffer::MsgBuffer, MsgRecord, MsgType},
    };

    async fn make_node() -> (SceneNodePtr, DateMsgNodePtr) {
        let chat = crate::app::node::create_chatview("chatview");
        let chat = chat.setup_null();
        let atom = &mut PropertyAtomicGuard::none();
        chat.set_property_f32(atom, Role::App, "font_size", 20.).unwrap();
        chat.set_property_f32(atom, Role::App, "line_height", 30.).unwrap();
        chat.set_property_f32(atom, Role::App, "message_spacing", 4.).unwrap();
        let prop = chat.get_property("rect").unwrap();
        prop.set_f32(atom, Role::App, 0, 0.).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.).unwrap();
        prop.set_f32(atom, Role::App, 2, 800.).unwrap();
        prop.set_f32(atom, Role::App, 3, 600.).unwrap();

        let mut wscale = crate::scene::SceneNode::new("w", crate::scene::SceneNodeType::Object);
        wscale
            .add_property(Property::new("scale", PropertyType::Float32, PropertySubType::Null))
            .unwrap();
        let wscale = wscale.setup_null();
        wscale.set_property_f32(atom, Role::App, "scale", 1.).unwrap();
        let window_scale = PropertyFloat32::wrap(&wscale, Role::Internal, "scale", 0).unwrap();

        let shared = super::super::SharedProps::wrap(&chat, window_scale);
        let node = create_datemsg_node("datemsg");
        let shared2 = shared.clone();
        let node = node.setup(|me| async move { DateMsgNode::new(me, shared2).await }).await;
        chat.link(node.clone());
        let Pimpl::DateMsgNode(ptr) = node.pimpl() else { panic!() };
        (chat, ptr.clone())
    }

    fn sep_rec(ts: Timestamp) -> MsgRecord {
        let payload = crate::ui::chatview::codec::encode_datemsg_payload(ts);
        MsgRecord { ts, id: MessageId([0; 32]), msg_type: MsgType::DateMsg, payload, height: 0. }
    }

    #[test]
    fn font_size_overrides_and_invalidates() {
        let (chat, node) = smol::block_on(make_node());
        let rec = sep_rec(1_756_000_000_000);
        let h1 = node.measure(&rec);
        // Inherits the chatview font size (20) with its 1.5 line ratio:
        // 30 + 4 spacing.
        assert!((h1 - 34.).abs() < 0.01, "{h1}");
        let builds = node.inner.lock().layout_builds;

        // A type-local font size is picked up, rebuilds the layout, and
        // shrinks the row box proportionally: 12 * 1.5 + 4.
        let atom = &mut PropertyAtomicGuard::none();
        let node2 = node.node().upgrade().unwrap();
        node2.set_property_f32(atom, Role::App, "font_size", 12.).unwrap();
        let h2 = node.measure(&rec);
        assert!(node.inner.lock().layout_builds > builds, "signature change rebuilds");
        assert!((h2 - 22.).abs() < 0.01, "shrunk line box: {h2}");

        let _ = chat;
    }
}
