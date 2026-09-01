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

//! The chatview record store: ordering, dedup, and height indexing.
//! Pure data — no rendering, no scene, no I/O.
//!
//! Records live in a slot arena (stable keys, O(1) insert/remove, keys
//! are recycled through a free list); `order` holds arena slots sorted
//! ascending by the `(timestamp, msg_id)` composite key, and `index`
//! maps the composite key to its slot. The composite (not the bare msg
//! id) is the dedup and identity key because derived records reuse
//! synthetic ids.
//!
//! Geometry: a Fenwick tree over heights, parallel to `order`, answers
//! total height, display positions, and viewport windows in O(log n).
//! The array is oldest-first so a live arrival (the hot path) appends
//! with `fenwick.push`. Public display-order APIs reverse the array
//! (display index 0 = newest = bottom of the screen), and
//! px-from-bottom queries convert through top-based offsets
//! (`total − x`).

use std::{collections::HashMap, ops::Range};

use chrono::{Local, TimeZone};

use super::{MessageId, MsgType, Timestamp};
use crate::{ui::chatview::codec, util::fenwick::Fenwick};

macro_rules! t { ($($arg:tt)*) => { trace!(target: "ui::chatview::buffer", $($arg)*); } }

/// A stable handle to a record in the slot arena. Only ever held by
/// `order`/`index` inside this module, so recycled slots cannot alias.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct SlotKey(u32);

/// Minimal slot arena with slotmap semantics: stable O(1) keys across
/// removals, free-list recycling. Implemented in-crate because the
/// design allows no new dependencies.
#[derive(Debug)]
pub(crate) struct Arena<T> {
    slots: Vec<Option<T>>,
    free: Vec<u32>,
}

impl<T> Default for Arena<T> {
    fn default() -> Self {
        Self { slots: vec![], free: vec![] }
    }
}

impl<T> Arena<T> {
    fn insert(&mut self, val: T) -> SlotKey {
        if let Some(idx) = self.free.pop() {
            self.slots[idx as usize] = Some(val);
            return SlotKey(idx)
        }
        self.slots.push(Some(val));
        SlotKey(self.slots.len() as u32 - 1)
    }

    fn remove(&mut self, key: SlotKey) -> Option<T> {
        let val = self.slots.get_mut(key.0 as usize)?.take()?;
        self.free.push(key.0);
        Some(val)
    }

    fn get(&self, key: SlotKey) -> Option<&T> {
        self.slots.get(key.0 as usize)?.as_ref()
    }

    fn get_mut(&mut self, key: SlotKey) -> Option<&mut T> {
        self.slots.get_mut(key.0 as usize)?.as_mut()
    }

    fn clear(&mut self) {
        self.slots.clear();
        self.free.clear();
    }
}

/// One loaded message. Layout is public to the loader and the codec;
/// per-type state (e.g. privmsg's `confirmed`) lives inside `payload`,
/// owned by the message type — not on this record.
#[derive(Debug, Clone)]
pub struct MsgRecord {
    /// Unix-millisecond send time; sorts messages into display order.
    pub ts: Timestamp,
    /// Unique identity; the zero id marks derived records.
    pub id: MessageId,
    /// The message's type; decides how `payload` is interpreted.
    pub msg_type: MsgType,
    /// Type-owned encoded state.
    pub payload: Vec<u8>,
    /// Last height reported by the owning type node, in px.
    pub height: f32,
}

impl MsgRecord {
    /// A record with no payload and zero height, for tests and as a
    /// base for builders.
    pub fn new(ts: Timestamp, id: MessageId, msg_type: MsgType) -> Self {
        Self { ts, id, msg_type, payload: vec![], height: 0. }
    }

    pub(crate) fn key(&self) -> (Timestamp, MessageId) {
        (self.ts, self.id)
    }
}
/// The record store. Internally `order` is sorted ascending by
/// `(ts, msg_id)` (oldest first); public display-order APIs are
/// newest-first, reversed.
#[derive(Debug)]
pub struct MsgBuffer {
    records: Arena<MsgRecord>,
    /// Arena slots sorted ascending by `(ts, msg_id)`; the last slot is
    /// the newest record (the bottom of the screen).
    order: Vec<SlotKey>,
    /// `(ts, msg_id)` -> arena slot. The dedup and identity map.
    index: HashMap<(Timestamp, MessageId), SlotKey>,
    /// `msg_id` -> arena slot, for records with unique ids (everything
    /// except derived types).
    id_index: HashMap<MessageId, SlotKey>,
    /// Cumulative heights over `order` (aligned to the ascending array).
    fenwick: Fenwick,
    /// Whether derived-record (date separator) maintenance runs.
    /// Production buffers keep this on; geometry/ordering tests that
    /// model records directly turn it off.
    separators: bool,
}

impl Default for MsgBuffer {
    fn default() -> Self {
        Self {
            records: Arena::default(),
            order: vec![],
            index: HashMap::new(),
            id_index: HashMap::new(),
            fenwick: Fenwick::empty(),
            separators: true,
        }
    }
}

impl MsgBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Disable derived-record maintenance (tests modeling records
    /// directly; separator behavior has its own tests).
    pub fn disable_separators(&mut self) {
        self.separators = false;
    }

    /// Number of loaded records (derived records included).
    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// Whether no records are loaded.
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// Drop every record.
    pub fn clear(&mut self) {
        self.records.clear();
        self.order.clear();
        self.index.clear();
        self.id_index.clear();
        self.fenwick = Fenwick::empty();
    }

    /// Insert a record at its `(ts, msg_id)` position. Returns false —
    /// leaving the buffer untouched — when a record with the same
    /// composite key is already loaded (dedup).
    ///
    /// A newest-position insert (the live-arrival hot path) appends in
    /// O(log n); any other position is structural and rebuilds the
    /// Fenwick in O(n) — batch those through [`MsgBuffer::insert_batch`].
    pub fn insert(&mut self, rec: MsgRecord) -> bool {
        let Some(slot) = self.insert_nofen(rec) else { return false };
        let pos = self.order.iter().position(|s| *s == slot).expect("freshly inserted slot");

        if self.order.last() == Some(&slot) {
            // Appended at the newest end: a plain Fenwick push.
            let height = self.records.get(slot).unwrap().height;
            self.fenwick.push(height);
        } else {
            self.rebuild_fenwick();
        }

        // Derived-record invariant: day-run heads get their separator.
        self.sync_separator_for_insert(pos);

        true
    }

    /// Batch insert with a single O(n) Fenwick rebuild covering the
    /// whole batch, for structural edits (loader backfill, filter
    /// reloads). Returns how many records were inserted (duplicates in
    /// the batch or against loaded records are rejected).
    pub fn insert_batch(&mut self, batch: impl IntoIterator<Item = MsgRecord>) -> usize {
        let mut count = 0;
        for rec in batch {
            if self.insert_nofen(rec).is_some() {
                count += 1;
            }
        }
        self.resync_separators();
        self.rebuild_fenwick();
        count
    }

    /// The ordering/index/dedup half of an insert, without touching the
    /// Fenwick tree. Returns the record's arena slot on success.
    fn insert_nofen(&mut self, rec: MsgRecord) -> Option<SlotKey> {
        let key = rec.key();
        if self.index.contains_key(&key) {
            t!("insert dedup rejected ts={} id={}", key.0, key.1);
            return None
        }

        // Ascending by key: the insertion position is the count of
        // records with a strictly smaller key.
        let pos = self.order.partition_point(|slot| {
            self.records.get(*slot).expect("dangling slot in order").key() < key
        });

        let derived = rec.msg_type.is_derived();
        let id = rec.id;
        let slot = self.records.insert(rec);
        self.order.insert(pos, slot);
        self.index.insert(key, slot);
        if !derived {
            self.id_index.insert(id, slot);
        }

        t!("insert ts={} id={} at pos {pos}", key.0, id);
        Some(slot)
    }

    /// Re-measure every loaded record (visible-first ordering is the
    /// caller's choice through `order_fn`) and rebuild the Fenwick once.
    /// The reflow protocol's bulk path.
    pub fn remeasure_all(&mut self, mut measure: impl FnMut(&MsgRecord) -> f32) {
        for slot in self.order.clone() {
            if let Some(rec) = self.records.get_mut(slot) {
                let h = measure(&*rec);
                rec.height = h;
            }
        }
        self.rebuild_fenwick();
    }

    /// One O(n) Fenwick pass from the current `order` heights.
    pub(crate) fn rebuild_fenwick(&mut self) {
        let mut heights = Vec::with_capacity(self.order.len());
        for slot in &self.order {
            heights.push(self.records.get(*slot).unwrap().height);
        }
        self.fenwick.rebuild(&heights);
        t!("fenwick rebuild over {} records", self.order.len());
    }

    /// Remove the record with this id (derived records are not in the
    /// id index; they are removed internally by slot). Returns false if
    /// no such record is loaded.
    pub fn remove(&mut self, id: &MessageId) -> bool {
        let Some(slot) = self.id_index.get(id).copied() else { return false };
        self.remove_slot(slot).is_some()
    }

    /// Structural removal by arena slot; also the derived-record path.
    fn remove_slot(&mut self, slot: SlotKey) -> Option<MsgRecord> {
        let rec = self.records.remove(slot)?;
        self.index.remove(&rec.key());
        if !rec.msg_type.is_derived() {
            self.id_index.remove(&rec.id);
        }
        self.order.retain(|s| *s != slot);
        if self.separators && !rec.msg_type.is_derived() {
            self.cleanup_separator_for_removal(rec.ts);
        }
        self.rebuild_fenwick();
        t!("removed ts={} id={}", rec.ts, rec.id);
        Some(rec)
    }

    /// Whether a record with this id is loaded.
    pub fn contains(&self, id: &MessageId) -> bool {
        self.id_index.contains_key(id)
    }

    /// The record with this id, if loaded.
    pub fn record(&self, id: &MessageId) -> Option<&MsgRecord> {
        self.records.get(*self.id_index.get(id)?)
    }

    /// Mutable access to the record with this id, if loaded.
    pub fn record_mut(&mut self, id: &MessageId) -> Option<&mut MsgRecord> {
        self.records.get_mut(*self.id_index.get(id)?)
    }

    /// The record at display position `idx` (0 = newest).
    pub fn record_at(&self, idx: usize) -> Option<&MsgRecord> {
        let f = self.order.len().checked_sub(1 + idx)?;
        self.records.get(*self.order.get(f)?)
    }

    /// Loaded records in display order (newest first).
    pub fn iter_display_order(&self) -> impl Iterator<Item = &MsgRecord> {
        self.order.iter().rev().filter_map(|slot| self.records.get(*slot))
    }

    /// The oldest loaded timestamp: the loader's resume point. None
    /// when nothing is loaded.
    pub fn oldest_ts(&self) -> Option<Timestamp> {
        let first = self.order.first()?;
        Some(self.records.get(*first).expect("dangling slot in order").ts)
    }

    /// Total px of loaded content.
    pub fn total_height(&self) -> f32 {
        self.fenwick.prefix(self.order.len())
    }

    /// Px from the content bottom up to the top of the record with
    /// this id (id-index backed; derived records share synthetic ids
    /// and are excluded — use [`MsgBuffer::pos_of_key`]).
    pub fn pos_of(&self, id: &MessageId) -> Option<f32> {
        let f = self.fenwick_idx(id)?;
        Some(self.total_height() - self.fenwick.prefix(f))
    }

    /// Px from the content bottom up to the top of the record with
    /// this composite key. Works for every record, derived ones
    /// included.
    pub fn pos_of_key(&self, key: &(Timestamp, MessageId)) -> Option<f32> {
        let f = self.fenwick_idx_key(key)?;
        Some(self.total_height() - self.fenwick.prefix(f))
    }

    /// Update a record's height; returns the delta (new − old) for
    /// scroll compensation, or None if the id is not loaded. O(log n)
    /// point update.
    pub fn set_height(&mut self, id: &MessageId, h: f32) -> Option<f32> {
        let slot = *self.id_index.get(id)?;
        self.set_height_slot(slot, h)
    }

    /// Composite-key height update (derived records included).
    pub fn set_height_key(&mut self, key: &(Timestamp, MessageId), h: f32) -> Option<f32> {
        let slot = *self.index.get(key)?;
        self.set_height_slot(slot, h)
    }

    /// The stored height of the record with this composite key.
    pub fn index_get_height(&self, key: &(Timestamp, MessageId)) -> Option<f32> {
        let slot = *self.index.get(key)?;
        self.records.get(slot).map(|rec| rec.height)
    }

    fn set_height_slot(&mut self, slot: SlotKey, h: f32) -> Option<f32> {
        let rec = self.records.get_mut(slot)?;
        let f = self.order.iter().position(|s| *s == slot).expect("record slot not in order");
        let delta = h - rec.height;
        if delta != 0. {
            self.fenwick.set(f, h);
        }
        rec.height = h;
        Some(delta)
    }

    /// Display-order range intersecting the viewport
    /// `[scroll, scroll + view_h)` in px from the content bottom.
    /// Half-open; display indices (0 = newest).
    pub fn visible_range(&self, scroll: f32, view_h: f32) -> Range<usize> {
        let n = self.order.len();
        // The viewport in top-based offsets (measured from the content
        // top): a is the top edge, b the bottom edge.
        let b = self.total_height() - scroll;
        let a = b - view_h;

        // Oldest-first index of the first record whose top edge is
        // above the viewport's top edge; records before it are older
        // and out of sight.
        let start_f = self.fenwick.lower_bound(a);
        // Even that record starts at/above the viewport's bottom edge:
        // the viewport sits below all loaded content from here on.
        if start_f >= n || self.fenwick.prefix(start_f) >= b {
            return n..n
        }

        // One past the oldest-first index of the last record starting
        // below the viewport's bottom edge, converted to display
        // positions.
        let end_f = self.fenwick.lower_bound_prefix(b);
        (n - end_f)..(n - start_f)
    }

    /// The record containing content px `content_y` measured from the
    /// content bottom (the message spacing belongs to the message it
    /// trails). None outside loaded content.
    pub fn record_at_y(&self, content_y: f32) -> Option<&MsgRecord> {
        let n = self.order.len();
        if n == 0 || content_y < 0. {
            return None
        }
        // The Fenwick accumulates from the top (oldest-first), so the
        // query converts to a top-based offset first — the same
        // conversion visible_range does.
        let total = self.total_height();
        if content_y >= total {
            return None
        }
        let f = self.fenwick.lower_bound(total - content_y);
        if f >= n {
            return None
        }
        self.record_at(n - 1 - f)
    }

    /// The local-midnight timestamp of a record's day. Falls back to
    /// the naive UTC conversion in zones/times where local midnight is
    /// nonexistent or ambiguous (DST transitions at 00:00) instead of
    /// panicking; ordering only needs a monotone day boundary.
    fn midnight_of(ts: Timestamp) -> Timestamp {
        let Some(dt) = Local.timestamp_millis_opt(ts as i64).single() else {
            return ts.saturating_sub(ts % 86_400_000)
        };
        let Some(naive) = dt.date_naive().and_hms_opt(0, 0, 0) else {
            return ts.saturating_sub(ts % 86_400_000)
        };
        match Local.from_local_datetime(&naive).single() {
            Some(local) => local.timestamp_millis() as u64,
            // Ambiguous/nonexistent midnight: pick the earliest mapping.
            None => match Local.from_local_datetime(&naive).earliest() {
                Some(local) => local.timestamp_millis() as u64,
                None => naive.and_utc().timestamp_millis() as u64,
            },
        }
    }

    /// A separator record for a day: synthetic `(midnight, zero id)`
    /// key. Because every message of the day has `ts >= midnight` and
    /// every older message has `ts < midnight`, the ordinary composite
    /// ordering places the separator exactly at the boundary of the
    /// day's run; the zero id only breaks the exact-midnight tie.
    fn separator_for(midnight: Timestamp) -> MsgRecord {
        MsgRecord {
            ts: midnight,
            id: MessageId([0; 32]),
            msg_type: MsgType::DateMsg,
            payload: codec::encode_datemsg_payload(midnight),
            height: 0.,
        }
    }

    fn has_separator(&self, midnight: Timestamp) -> bool {
        self.index.contains_key(&(midnight, MessageId([0; 32])))
    }

    /// Separator maintenance after a single record insert: a record
    /// whose older neighbor lives on another day starts a new day-run
    /// and gets its separator next to it.
    fn sync_separator_for_insert(&mut self, pos: usize) {
        let Some(slot) = self.order.get(pos).copied() else { return };
        let rec = self.records.get(slot).expect("dangling slot in order");
        if rec.msg_type.is_derived() {
            return
        }
        let midnight = Self::midnight_of(rec.ts);

        let starts_run = match self.order.get(pos.wrapping_sub(1)) {
            Some(&older_slot) if pos > 0 => {
                let older = self.records.get(older_slot).expect("dangling slot in order");
                !older.msg_type.is_derived() && Self::midnight_of(older.ts) != midnight
            }
            _ => true,
        };
        if self.separators && starts_run && !self.has_separator(midnight) {
            let sep = Self::separator_for(midnight);
            t!("separator for day {midnight}");
            self.insert(sep);
        }
    }

    /// Orphan cleanup after a removal: a day that lost its last message
    /// leaves its separator behind.
    fn cleanup_separator_for_removal(&mut self, ts: Timestamp) {
        let midnight = Self::midnight_of(ts);
        let day_still_populated = self.order.iter().any(|slot| {
            let rec = self.records.get(*slot).expect("dangling slot in order");
            !rec.msg_type.is_derived() && Self::midnight_of(rec.ts) == midnight
        });
        if !day_still_populated && self.has_separator(midnight) {
            let sep_slot = self.index[&(midnight, MessageId([0; 32]))];
            t!("orphan separator removed for day {midnight}");
            self.remove_slot(sep_slot);
        }
    }

    /// Full separator resync after structural batches: every maximal
    /// same-day run gets exactly one separator; separators of days with
    /// no messages are removed. O(n) alongside the batch rebuild.
    fn resync_separators(&mut self) {
        if !self.separators {
            return
        }
        let mut days = std::collections::HashSet::new();
        let mut seps = std::collections::HashSet::new();
        for slot in &self.order {
            let rec = self.records.get(*slot).expect("dangling slot in order");
            if rec.msg_type.is_derived() {
                seps.insert(rec.ts);
            } else {
                days.insert(Self::midnight_of(rec.ts));
            }
        }

        for midnight in days.iter() {
            if !seps.contains(midnight) {
                let sep = Self::separator_for(*midnight);
                t!("separator for day {midnight}");
                self.insert_nofen(sep);
            }
        }
        for midnight in seps {
            if !days.contains(&midnight) {
                let slot = self.index[&(midnight, MessageId([0; 32]))];
                self.remove_slot(slot);
                t!("orphan separator removed for day {midnight}");
            }
        }
    }

    /// Fenwick (ascending) index of the record with this id.
    fn fenwick_idx(&self, id: &MessageId) -> Option<usize> {
        let slot = *self.id_index.get(id)?;
        self.order.iter().position(|s| *s == slot)
    }

    /// Fenwick (ascending) index of the record with this composite key.
    fn fenwick_idx_key(&self, key: &(Timestamp, MessageId)) -> Option<usize> {
        let slot = *self.index.get(key)?;
        self.order.iter().position(|s| *s == slot)
    }

    /// Display-order position of the record with this id.
    pub(crate) fn position_of(&self, id: &MessageId) -> Option<usize> {
        Some(self.order.len() - 1 - self.fenwick_idx(id)?)
    }
}

/// The height-change scroll compensation rule. When a message entirely
/// below the viewport bottom (its top edge `msg_top` at or below
/// `scroll`) changes height by `delta`, the content the user is looking
/// at must not move: since scroll measures from the content bottom,
/// keeping the same content in view means adding `delta`. At `scroll ==
/// 0` (bottom pinned) there is nothing to hold — content grows upward
/// and the view auto-follows. Changes overlapping or above the viewport
/// are visible growth by design and are also left alone.
pub fn below_viewport_compensation(delta: f32, msg_top: f32, scroll: f32) -> f32 {
    if scroll > 0. && msg_top <= scroll {
        delta
    } else {
        0.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(ts: Timestamp, id_byte: u8) -> MsgRecord {
        MsgRecord::new(ts, MessageId([id_byte; 32]), MsgType::PrivMsg)
    }

    fn ids_in_order(buf: &MsgBuffer) -> Vec<u8> {
        let mut ids = vec![];
        for rec in buf.iter_display_order() {
            ids.push(rec.id.0[0]);
        }
        ids
    }

    #[test]
    fn insert_orders_descending() {
        let mut buf = MsgBuffer::new();
        buf.disable_separators();
        assert!(buf.insert(rec(300, b'a')));
        assert!(buf.insert(rec(100, b'b')));
        assert!(buf.insert(rec(200, b'c')));
        assert_eq!(ids_in_order(&buf), vec![b'a', b'c', b'b']);
        assert_eq!(buf.oldest_ts(), Some(100));
    }

    #[test]
    fn insert_newest_goes_to_front() {
        let mut buf = MsgBuffer::new();
        buf.disable_separators();
        buf.insert(rec(100, b'a'));
        buf.insert(rec(200, b'b'));
        buf.insert(rec(500, b'c'));
        assert_eq!(ids_in_order(&buf), vec![b'c', b'b', b'a']);
    }

    #[test]
    fn insert_older_goes_to_back() {
        let mut buf = MsgBuffer::new();
        buf.disable_separators();
        buf.insert(rec(200, b'b'));
        buf.insert(rec(300, b'c'));
        buf.insert(rec(50, b'a'));
        assert_eq!(ids_in_order(&buf), vec![b'c', b'b', b'a']);
        assert_eq!(buf.oldest_ts(), Some(50));
    }

    #[test]
    fn duplicate_insert_is_ignored() {
        let mut buf = MsgBuffer::new();
        buf.disable_separators();
        assert!(buf.insert(rec(100, b'a')));
        assert!(!buf.insert(rec(100, b'a')));
        assert_eq!(buf.len(), 1);
        assert_eq!(ids_in_order(&buf), vec![b'a']);
    }

    #[test]
    fn same_millisecond_coexists_ordered_by_id() {
        let mut buf = MsgBuffer::new();
        buf.disable_separators();
        assert!(buf.insert(rec(100, b'x')));
        assert!(buf.insert(rec(100, b'b')));
        assert!(buf.insert(rec(100, b'q')));
        assert_eq!(buf.len(), 3);
        assert_eq!(ids_in_order(&buf), vec![b'x', b'q', b'b']);
    }

    #[test]
    fn removal_updates_ordering() {
        let mut buf = MsgBuffer::new();
        buf.disable_separators();
        buf.insert(rec(100, b'a'));
        buf.insert(rec(200, b'b'));
        buf.insert(rec(300, b'c'));

        assert!(buf.remove(&MessageId([b'b'; 32])));
        assert_eq!(ids_in_order(&buf), vec![b'c', b'a']);
        assert!(!buf.contains(&MessageId([b'b'; 32])));
        assert!(!buf.remove(&MessageId([b'b'; 32])));

        assert!(buf.remove(&MessageId([b'c'; 32])));
        assert_eq!(ids_in_order(&buf), vec![b'a']);
        assert_eq!(buf.oldest_ts(), Some(100));
    }

    #[test]
    fn reinsert_after_removal_works() {
        let mut buf = MsgBuffer::new();
        buf.disable_separators();
        buf.insert(rec(100, b'a'));
        buf.insert(rec(200, b'b'));
        assert!(buf.remove(&MessageId([b'b'; 32])));
        assert!(buf.insert(rec(200, b'b')));
        assert_eq!(ids_in_order(&buf), vec![b'b', b'a']);
        assert_eq!(buf.position_of(&MessageId([b'b'; 32])), Some(0));
    }

    #[test]
    fn record_access_by_id() {
        let mut buf = MsgBuffer::new();
        buf.disable_separators();
        buf.insert(rec(100, b'a'));
        buf.record_mut(&MessageId([b'a'; 32])).unwrap().height = 42.;
        assert_eq!(buf.record(&MessageId([b'a'; 32])).unwrap().height, 42.);
        assert!(buf.record(&MessageId([b'z'; 32])).is_none());
    }

    #[test]
    fn clear_resets_everything() {
        let mut buf = MsgBuffer::new();
        buf.disable_separators();
        buf.insert(rec(100, b'a'));
        buf.insert(rec(200, b'b'));
        buf.clear();
        assert!(buf.is_empty());
        assert_eq!(buf.oldest_ts(), None);
        assert!(buf.insert(rec(100, b'a')));
        assert_eq!(ids_in_order(&buf), vec![b'a']);
    }

    #[test]
    fn derived_records_share_synthetic_ids() {
        let mut buf = MsgBuffer::new();
        buf.disable_separators();
        let sep1 = MsgRecord::new(1000, MessageId([0; 32]), MsgType::DateMsg);
        let sep2 = MsgRecord::new(2000, MessageId([0; 32]), MsgType::DateMsg);
        assert!(buf.insert(sep1));
        assert!(buf.insert(sep2));
        assert_eq!(buf.len(), 2);
        // The zero id is not in the id index: derived lookups and
        // public removal go through real message ids only.
        assert!(!buf.contains(&MessageId([0; 32])));
        assert!(!buf.remove(&MessageId([0; 32])));
        assert_eq!(buf.oldest_ts(), Some(1000));
    }

    #[test]
    fn randomized_inserts_match_sorted_reference() {
        use rand::{rngs::StdRng, Rng, SeedableRng};
        let mut rng = StdRng::seed_from_u64(0xBEEF);
        for _ in 0..50 {
            let n = rng.gen_range(0..200);
            let mut keys = vec![];
            for _ in 0..n {
                let key = (rng.gen_range(0..10_000u64), rng.gen_range(0..255u8));
                keys.push(key);
            }
            let mut buf = MsgBuffer::new();
            buf.disable_separators();
            for (ts, id) in &keys {
                buf.insert(rec(*ts, *id));
            }

            keys.sort_unstable();
            keys.dedup();
            keys.reverse();
            assert_eq!(buf.len(), keys.len(), "loaded count");
            let mut expect = vec![];
            for (_, id) in &keys {
                expect.push(*id);
            }
            assert_eq!(ids_in_order(&buf), expect, "display order");
            assert_eq!(buf.oldest_ts(), keys.last().map(|(ts, _)| *ts));
        }
    }

    fn id(b: u8) -> MessageId {
        MessageId([b; 32])
    }

    /// A unique id from a counter, for randomized tests.
    fn num_id(n: u64) -> MessageId {
        let mut bytes = [0u8; 32];
        bytes[..8].copy_from_slice(&n.to_be_bytes());
        MessageId(bytes)
    }

    fn hrec(ts: Timestamp, id_byte: u8, height: f32) -> MsgRecord {
        let mut r = rec(ts, id_byte);
        r.height = height;
        r
    }

    fn hrec_num(ts: Timestamp, id_num: u64, height: f32) -> MsgRecord {
        let mut r = MsgRecord::new(ts, num_id(id_num), MsgType::PrivMsg);
        r.height = height;
        r
    }

    /// Brute-force visible range over display-order heights: display
    /// record i occupies content px [cum(i) - h_i, cum(i)) from the
    /// bottom; visible iff it intersects [scroll, scroll + vh).
    fn brute_visible(heights: &[f32], scroll: f32, vh: f32) -> Range<usize> {
        let mut start = heights.len();
        let mut end = heights.len();
        let mut cum = 0.;
        for (i, h) in heights.iter().enumerate() {
            let bottom = cum;
            cum += h;
            if start == heights.len() && cum > scroll {
                start = i;
            }
            if bottom >= scroll + vh {
                end = i;
                break
            }
        }
        start..end
    }

    #[test]
    fn geometry_basics() {
        // Display order (newest first): c(30) b(20) a(10); total 60.
        let mut buf = MsgBuffer::new();
        buf.disable_separators();
        buf.insert(hrec(100, b'a', 10.));
        buf.insert(hrec(200, b'b', 20.));
        buf.insert(hrec(300, b'c', 30.));

        assert_eq!(buf.total_height(), 60.);
        assert_eq!(buf.pos_of(&id(b'a')), Some(60.));
        assert_eq!(buf.pos_of(&id(b'b')), Some(50.));
        assert_eq!(buf.pos_of(&id(b'c')), Some(30.));
        assert_eq!(buf.pos_of(&id(b'z')), None);

        assert_eq!(buf.visible_range(0., 60.), 0..3);
        assert_eq!(buf.visible_range(0., 35.), 0..2);
        assert_eq!(buf.visible_range(10., 20.), 0..1);
        assert_eq!(buf.visible_range(30., 30.), 1..3);
        assert_eq!(buf.visible_range(60., 10.), 3..3);

        // record_at agrees with display iteration.
        assert_eq!(buf.record_at(0).unwrap().id, id(b'c'));
        assert_eq!(buf.record_at(2).unwrap().id, id(b'a'));
        assert!(buf.record_at(3).is_none());
    }

    #[test]
    fn set_height_reports_delta_and_updates_geometry() {
        let mut buf = MsgBuffer::new();
        buf.disable_separators();
        buf.insert(hrec(100, b'a', 10.));
        buf.insert(hrec(200, b'b', 20.));
        buf.insert(hrec(300, b'c', 30.));

        assert_eq!(buf.set_height(&id(b'b'), 35.), Some(15.));
        assert_eq!(buf.total_height(), 75.);
        assert_eq!(buf.pos_of(&id(b'a')), Some(75.));
        assert_eq!(buf.pos_of(&id(b'b')), Some(65.));
        assert_eq!(buf.pos_of(&id(b'c')), Some(30.));

        assert_eq!(buf.set_height(&id(b'b'), 35.), Some(0.));
        assert_eq!(buf.set_height(&id(b'z'), 5.), None);
    }

    #[test]
    fn insert_batch_matches_single_inserts() {
        let batch: Vec<MsgRecord> = vec![
            hrec(300, b'c', 30.),
            hrec(100, b'a', 10.),
            hrec(500, b'e', 50.),
            hrec(200, b'b', 20.),
        ];
        let mut batched = MsgBuffer::new();
        batched.disable_separators();
        assert_eq!(batched.insert_batch(batch), 4);
        let mut single = MsgBuffer::new();
        single.disable_separators();
        single.insert(hrec(300, b'c', 30.));
        single.insert(hrec(100, b'a', 10.));
        single.insert(hrec(500, b'e', 50.));
        single.insert(hrec(200, b'b', 20.));

        for buf in [&batched, &single] {
            assert_eq!(buf.total_height(), 110.);
            assert_eq!(ids_in_order(buf), vec![b'e', b'c', b'b', b'a']);
            assert_eq!(buf.visible_range(0., 60.), 0..2);
        }

        // Duplicates against loaded records are rejected by the batch too.
        assert_eq!(batched.insert_batch(vec![hrec(200, b'b', 99.)]), 0);
        assert_eq!(batched.record(&id(b'b')).unwrap().height, 20.);
    }

    #[test]
    fn randomized_geometry_against_linear_scan() {
        use rand::{rngs::StdRng, seq::SliceRandom, Rng, SeedableRng};
        let mut rng = StdRng::seed_from_u64(0xD1CE);

        for _ in 0..30 {
            let mut buf = MsgBuffer::new();
            buf.disable_separators();
            // Reference model: (ts, id) -> height, kept sorted ascending.
            let mut model: Vec<(Timestamp, u64, f32)> = vec![];
            let mut next_id = 0u64;

            for _ in 0..400 {
                match rng.gen_range(0..6) {
                    0 | 1 | 2 => {
                        let ts = rng.gen_range(0..5000u64);
                        let id_num = next_id;
                        next_id += 1;
                        let h = rng.gen_range(0..80) as f32;
                        buf.insert(hrec_num(ts, id_num, h));
                        if !model.iter().any(|(t, i, _)| *t == ts && *i == id_num) {
                            model.push((ts, id_num, h));
                            model.sort_unstable_by_key(|(t, i, _)| (*t, *i));
                        }
                    }
                    3 => {
                        // Batch of older records (backfill-shaped).
                        let mut batch = vec![];
                        for _ in 0..rng.gen_range(1..8) {
                            let ts = rng.gen_range(0..5000u64);
                            let id_num = next_id;
                            next_id += 1;
                            let h = rng.gen_range(0..80) as f32;
                            batch.push(hrec_num(ts, id_num, h));
                            model.push((ts, id_num, h));
                        }
                        model.sort_unstable_by_key(|(t, i, _)| (*t, *i));
                        model.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
                        buf.insert_batch(batch);
                    }
                    4 => {
                        if let Some(&(_, id_num, _)) = model.choose(&mut rng) {
                            let new_h = rng.gen_range(0..120) as f32;
                            buf.set_height(&num_id(id_num), new_h);
                            for (_, i, h) in model.iter_mut() {
                                if *i == id_num {
                                    *h = new_h;
                                }
                            }
                        }
                    }
                    _ => {
                        if !model.is_empty() {
                            let idx = rng.gen_range(0..model.len());
                            let (_, id_num, _) = model[idx];
                            model.remove(idx);
                            assert!(buf.remove(&num_id(id_num)));
                        }
                    }
                }

                // Display-order heights from the ascending model.
                let mut disp = vec![];
                for (_, _, h) in model.iter().rev() {
                    disp.push(*h);
                }
                let mut total = 0.;
                for h in &disp {
                    total += h;
                }
                assert!((buf.total_height() - total).abs() < 1e-2, "total_height");
                assert_eq!(buf.len(), disp.len(), "loaded count");

                let mut cum = 0.;
                for (d, h) in disp.iter().enumerate() {
                    let record = buf.record_at(d).unwrap();
                    assert!((buf.pos_of(&record.id).unwrap() - (cum + h)).abs() < 1e-2, "pos_of");
                    cum += h;
                }

                let mut scrolls = vec![0., total / 2., total];
                for _ in 0..6 {
                    scrolls.push(rng.gen_range(0f32..total + 1.));
                }
                for scroll in scrolls {
                    let vh = rng.gen_range(0f32..600.);
                    let expect = brute_visible(&disp, scroll, vh);
                    let got = buf.visible_range(scroll, vh);
                    assert_eq!(got, expect, "visible_range(scroll={scroll}, vh={vh})");
                }
            }
        }
    }

    /// Timestamps on distinct local days, in unix ms.
    fn day_ts(day_offset: i64, hour: u32, min: u32) -> Timestamp {
        use chrono::NaiveDate;
        let date =
            NaiveDate::from_ymd_opt(2026, 8, 29).unwrap() + chrono::Duration::days(day_offset);
        let dt = date.and_hms_opt(hour, min, 0).unwrap();
        Local.from_local_datetime(&dt).unwrap().timestamp_millis() as u64
    }

    #[test]
    fn separators_appear_at_day_boundaries() {
        let mut buf = MsgBuffer::new();
        // Two messages on day 0, one on day 1.
        buf.insert(rec(day_ts(0, 10, 0), b'a'));
        buf.insert(rec(day_ts(0, 11, 0), b'b'));
        buf.insert(rec(day_ts(1, 9, 0), b'c'));

        let kinds: Vec<(u8, bool)> =
            buf.iter_display_order().map(|r| (r.id.0[0], r.msg_type.is_derived())).collect();
        // Display order (newest first): c, [sep day1], b, a, [sep day0].
        assert_eq!(kinds, vec![(b'c', false), (0, true), (b'b', false), (b'a', false), (0, true)]);

        // The separator keys are the local midnights.
        let seps: Vec<Timestamp> =
            buf.iter_display_order().filter(|r| r.msg_type.is_derived()).map(|r| r.ts).collect();
        assert_eq!(
            seps,
            vec![MsgBuffer::midnight_of(day_ts(1, 9, 0)), MsgBuffer::midnight_of(day_ts(0, 10, 0))]
        );
    }

    #[test]
    fn record_at_y_covers_derived_separators() {
        // A day-1 message, its separator, and a day-0 message; the
        // separator must be selectable like any record.
        let mut buf = MsgBuffer::new();
        buf.insert(hrec(day_ts(1, 9, 0), b'c', 30.));
        buf.insert(hrec(day_ts(0, 10, 0), b'a', 20.));

        let sep = buf
            .iter_display_order()
            .find(|r| r.msg_type.is_derived())
            .expect("separator present")
            .clone();
        assert!(buf.set_height_key(&(sep.ts, sep.id), 34.).is_some());

        // Walk the whole content span; the separator must be resolved
        // somewhere.
        let total = buf.total_height();
        let mut found = false;
        let mut y = 0.;
        while y < total {
            if let Some(rec) = buf.record_at_y(y) {
                if rec.id == sep.id {
                    found = true;
                    break
                }
            }
            y += 1.;
        }
        assert!(found, "separator never resolved by record_at_y");
    }

    #[test]
    fn record_at_y_resolves_the_on_screen_line() {
        // Ascending [a(10), b(20), c(30)], total 60: c occupies
        // bottom-based [0,30], b [30,50], a [50,60].
        let mut buf = MsgBuffer::new();
        buf.disable_separators();
        buf.insert(hrec(100, b'a', 10.));
        buf.insert(hrec(200, b'b', 20.));
        buf.insert(hrec(300, b'c', 30.));

        assert_eq!(buf.record_at_y(5.).unwrap().id.0[0], b'c');
        assert_eq!(buf.record_at_y(29.).unwrap().id.0[0], b'c');
        assert_eq!(buf.record_at_y(31.).unwrap().id.0[0], b'b');
        assert_eq!(buf.record_at_y(49.).unwrap().id.0[0], b'b');
        assert_eq!(buf.record_at_y(55.).unwrap().id.0[0], b'a');
        assert!(buf.record_at_y(61.).is_none());
        assert!(buf.record_at_y(-1.).is_none());
    }

    #[test]
    fn remeasure_all_rebuilds_geometry_from_new_heights() {
        let mut buf = MsgBuffer::new();
        buf.disable_separators();
        buf.insert(hrec(100, b'a', 10.));
        buf.insert(hrec(200, b'b', 20.));
        buf.insert(hrec(300, b'c', 30.));
        assert_eq!(buf.total_height(), 60.);

        // The reflow protocol's bulk path: every height replaced, one
        // Fenwick rebuild, geometry consistent.
        buf.remeasure_all(|rec| rec.height * 2.);
        assert_eq!(buf.total_height(), 120.);
        assert_eq!(buf.pos_of(&MessageId([b'a'; 32])), Some(120.));
        assert_eq!(buf.pos_of(&MessageId([b'b'; 32])), Some(100.));
        assert_eq!(buf.pos_of(&MessageId([b'c'; 32])), Some(60.));
        // Viewport [0, 60) holds only c (b's bottom edge sits exactly
        // at 60, outside the half-open window).
        assert_eq!(buf.visible_range(0., 60.), 0..1);
    }

    #[test]
    fn same_day_insert_adds_no_second_separator() {
        let mut buf = MsgBuffer::new();
        for i in 0..5 {
            assert!(buf.insert(rec(day_ts(0, 10, i), b'a' + i as u8)));
        }
        let sep_count = buf.iter_display_order().filter(|r| r.msg_type.is_derived()).count();
        assert_eq!(sep_count, 1);
    }

    #[test]
    fn deleting_a_days_only_message_removes_its_separator() {
        let mut buf = MsgBuffer::new();
        buf.insert(rec(day_ts(0, 10, 0), b'a'));
        buf.insert(rec(day_ts(1, 9, 0), b'b'));
        buf.insert(rec(day_ts(1, 10, 0), b'c'));
        assert_eq!(buf.iter_display_order().filter(|r| r.msg_type.is_derived()).count(), 2);

        // Remove day 0's only message: its separator must go too.
        assert!(buf.remove(&MessageId([b'a'; 32])));
        let kinds: Vec<(u8, bool)> =
            buf.iter_display_order().map(|r| (r.id.0[0], r.msg_type.is_derived())).collect();
        assert_eq!(kinds, vec![(b'c', false), (b'b', false), (0, true)]);

        // Removing one of day 1's messages keeps its separator.
        assert!(buf.remove(&MessageId([b'c'; 32])));
        let kinds: Vec<(u8, bool)> =
            buf.iter_display_order().map(|r| (r.id.0[0], r.msg_type.is_derived())).collect();
        assert_eq!(kinds, vec![(b'b', false), (0, true)]);
    }

    #[test]
    fn batch_insert_syncs_separators() {
        let mut buf = MsgBuffer::new();
        let batch = vec![
            rec(day_ts(2, 23, 0), b'x'),
            rec(day_ts(1, 8, 0), b'y'),
            rec(day_ts(0, 12, 0), b'z'),
            rec(day_ts(2, 1, 0), b'w'),
        ];
        assert_eq!(buf.insert_batch(batch), 4);
        assert_eq!(buf.iter_display_order().filter(|r| r.msg_type.is_derived()).count(), 3);

        // Display order: x w [sep d2] y [sep d1] z [sep d0].
        let kinds: Vec<u8> = buf
            .iter_display_order()
            .map(|r| if r.msg_type.is_derived() { b'|' } else { r.id.0[0] })
            .collect();
        assert_eq!(kinds, vec![b'x', b'w', b'|', b'y', b'|', b'z', b'|']);
    }

    #[test]
    fn clear_drops_separators() {
        let mut buf = MsgBuffer::new();
        buf.insert(rec(day_ts(0, 10, 0), b'a'));
        buf.insert(rec(day_ts(1, 9, 0), b'b'));
        buf.clear();
        assert!(buf.is_empty());
        buf.insert(rec(day_ts(1, 9, 0), b'b'));
        let sep_count = buf.iter_display_order().filter(|r| r.msg_type.is_derived()).count();
        assert_eq!(sep_count, 1, "separator re-derives after clear");
    }

    #[test]
    fn compensation_below_inside_above_and_pinned() {
        // Viewport [50, 80) in content px from the bottom.
        let scroll = 50.;

        // Message entirely below the viewport bottom: top <= scroll.
        assert_eq!(below_viewport_compensation(10., 40., scroll), 10.);
        assert_eq!(below_viewport_compensation(10., 50., scroll), 10.);
        // Message overlapping the viewport: top > scroll.
        assert_eq!(below_viewport_compensation(10., 60., scroll), 0.);
        // Message entirely above the viewport.
        assert_eq!(below_viewport_compensation(10., 90., scroll), 0.);
        // Bottom pinned: no adjustment even for below-viewport growth.
        assert_eq!(below_viewport_compensation(10., 40., 0.), 0.);
        // Shrinking below the viewport also compensates (negative delta).
        assert_eq!(below_viewport_compensation(-4., 40., scroll), -4.);
    }
}
