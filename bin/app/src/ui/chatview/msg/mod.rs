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

//! The chatview message-type contract and registry.
//!
//! Every message type is a fixed variant of [`MsgType`] (no factories,
//! no placeholders; unknown type ids panic at decode). Each variant is
//! served by exactly one type node — a scene sub-node of the chatview
//! carrying the type's styling properties, signals, and lifecycle
//! methods — plus per-id message instances owned by that node. The
//! instances split into CPU-only state (layouts, measured heights,
//! hit rects — unit-testable without a GPU) and renderer-bound work
//! (mesh/texture caches), which stays at the draw edge.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Weak},
};

use async_lock::Mutex as AsyncMutex;
use url::Url;

use crate::{
    app::node::{create_datemsg_node, create_filemsg_node, create_privmsg_node},
    gfx::{DrawInstruction, Point, Renderer},
    prop::{PropertyColor, PropertyFloat32, PropertyPtr, PropertyRect, Role},
    scene::{Pimpl, SceneNodePtr, SceneNodeWeak},
    util::i18n::I18nBabelFish,
};

use super::{
    buffer::MsgBuffer, loader::Loader, ChatView, MessageId, MsgRecord, MsgType, Timestamp,
};

pub mod datemsg;
pub mod filemsg;
pub mod privmsg;
pub use datemsg::{DateMsgNode, DateMsgNodePtr};
pub use filemsg::{FileMsgNode, FileMsgNodePtr};
pub use privmsg::{PrivMsgNode, PrivMsgNodePtr};

/// Live property handles for the styling shared by every message type,
/// defined once on the chatview node and handed to each type node's
/// constructor. A type node overrides anything it defines itself.
#[derive(Clone)]
pub struct SharedProps {
    pub font_size: PropertyFloat32,
    pub timestamp_font_size: PropertyFloat32,
    pub timestamp_width: PropertyFloat32,
    pub line_height: PropertyFloat32,
    pub message_spacing: PropertyFloat32,
    pub baseline: PropertyFloat32,
    pub timestamp_color: PropertyColor,
    pub text_color: PropertyColor,
    pub hi_bg_color: PropertyColor,
    pub window_scale: PropertyFloat32,
    /// The chatview's rect; `.w` is the wrap width.
    pub rect: PropertyRect,
}

impl SharedProps {
    /// Wrap the shared styling properties off the chatview scene node.
    pub fn wrap(chatview_node: &SceneNodePtr, window_scale: PropertyFloat32) -> Self {
        let font_size = PropertyFloat32::wrap(chatview_node, Role::Internal, "font_size", 0)
            .expect("chatview font_size");
        let timestamp_font_size =
            PropertyFloat32::wrap(chatview_node, Role::Internal, "timestamp_font_size", 0)
                .expect("chatview timestamp_font_size");
        let timestamp_width =
            PropertyFloat32::wrap(chatview_node, Role::Internal, "timestamp_width", 0)
                .expect("chatview timestamp_width");
        let line_height = PropertyFloat32::wrap(chatview_node, Role::Internal, "line_height", 0)
            .expect("chatview line_height");
        let message_spacing =
            PropertyFloat32::wrap(chatview_node, Role::Internal, "message_spacing", 0)
                .expect("chatview message_spacing");
        let baseline = PropertyFloat32::wrap(chatview_node, Role::Internal, "baseline", 0)
            .expect("chatview baseline");
        let timestamp_color = PropertyColor::wrap(chatview_node, Role::Internal, "timestamp_color")
            .expect("chatview timestamp_color");
        let text_color = PropertyColor::wrap(chatview_node, Role::Internal, "text_color")
            .expect("chatview text_color");
        let hi_bg_color = PropertyColor::wrap(chatview_node, Role::Internal, "hi_bg_color")
            .expect("chatview hi_bg_color");
        let rect =
            PropertyRect::wrap(chatview_node, Role::Internal, "rect").expect("chatview rect");

        Self {
            font_size,
            timestamp_font_size,
            timestamp_width,
            line_height,
            message_spacing,
            baseline,
            timestamp_color,
            text_color,
            hi_bg_color,
            window_scale,
            rect,
        }
    }
}

/// What a hit inside a message resolved to, in message-local
/// coordinates.
#[derive(Debug, Clone, PartialEq)]
pub enum Hit {
    Url(String),
    Nick(String),
    /// The collapsed/expanded toggle affordance of a capped message.
    Expand,
    /// A file message's activation (download request) target.
    File(Url),
}

/// How a record's draw instructions must be emitted: inline in the
/// chatview's call, or as a sibling draw call clipped to `clip_h`
/// (collapsed long messages).
pub enum DrawOutcome {
    Inline(Vec<DrawInstruction>),
    Clipped { instrs: Vec<DrawInstruction>, clip_h: f32 },
}

/// The per-type node registry: hardcoded enum dispatch, one stable
/// node per type, created with the chatview and never recreated by
/// buffer changes (channel switch, load, eviction).
pub struct TypeNodes {
    pub privmsg: PrivMsgNodePtr,
    pub datemsg: DateMsgNodePtr,
    pub filemsg: FileMsgNodePtr,
}

impl TypeNodes {
    /// Create the type sub-nodes as children of the chatview node.
    pub async fn new(
        chatview_node: &SceneNodePtr,
        shared: SharedProps,
        i18n: I18nBabelFish,
        loader: Arc<Loader>,
        buffer: Arc<AsyncMutex<MsgBuffer>>,
        chat: Weak<ChatView>,
    ) -> Self {
        let node = create_privmsg_node("privmsg");
        let shared2 = shared.clone();
        let loader2 = loader.clone();
        let buffer2 = buffer.clone();
        let chat2 = chat.clone();
        let node = node
            .setup(|me| async move { PrivMsgNode::new(me, shared2, loader2, buffer2, chat2).await })
            .await;
        let privmsg = node_ref_privmsg(&node);
        chatview_node.link(node);

        let node = create_datemsg_node("datemsg");
        let shared2 = shared.clone();
        let node = node.setup(|me| async move { DateMsgNode::new(me, shared2).await }).await;
        let datemsg = node_ref_datemsg(&node);
        chatview_node.link(node);

        let node = create_filemsg_node("filemsg");
        let shared2 = shared.clone();
        let i18n2 = i18n.clone();
        let loader2 = loader.clone();
        let buffer2 = buffer.clone();
        let node = node
            .setup(|me| async move {
                FileMsgNode::new(me, shared2, i18n2, loader2, buffer2, chat).await
            })
            .await;
        let filemsg = node_ref_filemsg(&node);
        chatview_node.link(node);

        Self { privmsg, datemsg, filemsg }
    }

    /// Measure a record: materialize its instance if needed (CPU-only
    /// layout work) and return its height. Used by the loader while
    /// collecting a batch, so heights exist before geometry is built.
    pub fn measure(&self, rec: &MsgRecord) -> f32 {
        match rec.msg_type {
            MsgType::PrivMsg => self.privmsg.measure(rec),
            MsgType::DateMsg => self.datemsg.measure(rec),
            MsgType::FileMsg => self.filemsg.measure(rec),
        }
    }

    /// Whether the record's instance currently holds rendered state.
    pub fn is_materialized(&self, rec: &MsgRecord) -> bool {
        match rec.msg_type {
            MsgType::PrivMsg => self.privmsg.is_materialized(rec),
            MsgType::DateMsg => self.datemsg.is_materialized(rec),
            MsgType::FileMsg => self.filemsg.is_materialized(rec),
        }
    }

    /// Ensure the instance exists (e.g. rematerialized after release).
    /// Returns the measured height, which the caller flows back into
    /// the buffer when it differs from the record's stored height.
    pub fn ensure_materialized(&self, rec: &MsgRecord) -> f32 {
        match rec.msg_type {
            MsgType::PrivMsg => self.privmsg.measure(rec),
            MsgType::DateMsg => self.datemsg.measure(rec),
            MsgType::FileMsg => self.filemsg.measure(rec),
        }
    }

    /// Drop every instance's rendered state (channel switch, reflow).
    pub fn release_all(&self) {
        self.privmsg.release_all();
        self.datemsg.release_all();
        self.filemsg.release_all();
    }

    /// Rebuild rendered state for every instance from live props.
    pub fn regen_all(&self) {
        self.privmsg.regen_all();
        self.datemsg.regen_all();
        self.filemsg.regen_all();
    }

    /// Renderer-bound draw instructions for a materialized record, in
    /// message-local coordinates (y grows downward from its top edge).
    pub fn draw(&self, rec: &MsgRecord, renderer: &Renderer) -> DrawOutcome {
        match rec.msg_type {
            MsgType::PrivMsg => self.privmsg.draw(rec, renderer),
            MsgType::DateMsg => DrawOutcome::Inline(self.datemsg.draw(rec, renderer)),
            MsgType::FileMsg => self.filemsg.draw(rec, renderer),
        }
    }

    /// Clipboard contribution when selected (None = nothing copied).
    pub fn copy_text(&self, rec: &MsgRecord) -> Option<String> {
        match rec.msg_type {
            MsgType::PrivMsg => self.privmsg.copy_text(rec),
            MsgType::DateMsg => self.datemsg.copy_text(rec),
            MsgType::FileMsg => self.filemsg.copy_text(rec),
        }
    }

    /// Hit dispatch (urls, nicks, buttons) in message-local coordinates.
    pub fn hit_test(&self, rec: &MsgRecord, pos: Point) -> Option<Hit> {
        match rec.msg_type {
            MsgType::PrivMsg => self.privmsg.hit_test(rec, pos),
            MsgType::FileMsg => self.filemsg.hit_test(rec, pos),
            _ => None,
        }
    }

    /// Release out-of-window instances beyond the LRU budget (the
    /// virtualization sweep; called from the draw path).
    pub fn sweep(&self, keep: &HashSet<privmsg::InstKey>, budget: usize) {
        self.privmsg.sweep(keep, budget);
        self.datemsg.sweep(keep, budget);
        self.filemsg.sweep(keep, budget);
    }
}

/// The soft-window + LRU budget policy: window members are always
/// kept; of the rest, the `budget` most recently touched survive and
/// older instances are released. Pure — unit-tested standalone.
pub fn evict_beyond<K: std::hash::Hash + Eq + Clone>(
    keep: &std::collections::HashSet<K>,
    touches: &HashMap<K, u64>,
    budget: usize,
) -> Vec<K> {
    let mut candidates: Vec<(K, u64)> = vec![];
    for (key, touch) in touches {
        if !keep.contains(key) {
            candidates.push((key.clone(), *touch));
        }
    }
    if candidates.len() <= budget {
        return vec![]
    }
    // Oldest first; release all but the newest `budget`.
    candidates.sort_unstable_by_key(|(_, touch)| *touch);
    let excess = candidates.len() - budget;
    let mut releases = vec![];
    for (key, _) in candidates.into_iter().take(excess) {
        releases.push(key);
    }
    releases
}

/// Pull the `PrivMsgNodePtr` back out of the set-up scene node.
fn node_ref_privmsg(node: &SceneNodePtr) -> PrivMsgNodePtr {
    let Pimpl::PrivMsgNode(ptr) = node.pimpl() else { panic!("privmsg node pimpl") };
    ptr.clone()
}

/// Pull the `DateMsgNodePtr` back out of the set-up scene node.
fn node_ref_datemsg(node: &SceneNodePtr) -> DateMsgNodePtr {
    let Pimpl::DateMsgNode(ptr) = node.pimpl() else { panic!("datemsg node pimpl") };
    ptr.clone()
}

/// Pull the `FileMsgNodePtr` back out of the set-up scene node.
fn node_ref_filemsg(node: &SceneNodePtr) -> FileMsgNodePtr {
    let Pimpl::FileMsgNode(ptr) = node.pimpl() else { panic!("filemsg node pimpl") };
    ptr.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(n: u64) -> privmsg::InstKey {
        (n, MessageId([n as u8; 32]))
    }

    #[test]
    fn evict_beyond_keeps_window_members() {
        let mut keep = HashSet::new();
        keep.insert(key(1));
        keep.insert(key(2));
        let mut touches = HashMap::new();
        for n in 0..10u64 {
            touches.insert(key(n), n);
        }
        // Window members are never released, whatever the budget.
        assert!(evict_beyond(&keep, &touches, 0).iter().all(|k| !keep.contains(k)));
        let releases = evict_beyond(&keep, &touches, 0);
        assert_eq!(releases.len(), 8);
    }

    #[test]
    fn evict_beyond_respects_budget() {
        let keep = HashSet::new();
        let mut touches = HashMap::new();
        for n in 0..10u64 {
            touches.insert(key(n), n);
        }
        // Under budget: nothing released.
        assert!(evict_beyond(&keep, &touches, 12).is_empty());
        // Budget 3 keeps the 3 most recently touched (7, 8, 9).
        let mut releases = evict_beyond(&keep, &touches, 3);
        releases.sort_unstable_by_key(|(n, _)| *n);
        assert_eq!(releases, vec![key(0), key(1), key(2), key(3), key(4), key(5), key(6)]);
    }

    #[test]
    fn evict_beyond_releases_oldest_first() {
        let keep = HashSet::new();
        let mut touches = HashMap::new();
        // Insertion order is not access order.
        touches.insert(key(0), 90);
        touches.insert(key(1), 10);
        touches.insert(key(2), 50);
        let releases = evict_beyond(&keep, &touches, 1);
        // The two oldest go oldest-first; key 0 (touch 90) fills the budget.
        assert_eq!(releases, vec![key(1), key(2)]);
    }
}
