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

//! The chatview background loader: the single kvdb-owning pipeline.
//!
//! One async task owns all kvdb access for its chatview and maintains
//! the coverage invariant: the loaded region always includes the live
//! bottom and extends far enough above the viewport to cover
//! `viewport + preload margin`. Wakes coalesce through a bitset of
//! reasons; the pump always just restores the invariant, whatever the
//! trigger — except ChannelSwitch/FilterChange, which clear the buffer
//! first. Records are decoded with the tagged codec (corrupt entries
//! panic loudly), filtered here (kvdb → filter → buffer), and inserted
//! as one batch per pump with a single Fenwick rebuild.

use async_lock::Mutex as AsyncMutex;
use darkfi::system::CondVar;
use kvdb_overlay::Tree;
use parking_lot::Mutex as SyncMutex;
use std::sync::{
    atomic::{AtomicU8, Ordering},
    Arc,
};

use super::{codec, MessageId, MsgBuffer, MsgRecord, MsgType, RedrawTrigger, Timestamp};

macro_rules! t { ($($arg:tt)*) => { trace!(target: "ui::chatview::loader", $($arg)*); } }
macro_rules! i { ($($arg:tt)*) => { info!(target: "ui::chatview::loader", $($arg)*); } }

/// Preload margin above the viewport, in viewport heights.
const PRELOAD_MARGIN_FRAC: f32 = 1.;
/// Records per pump batch at most. Until heights are measured by the
/// type framework the height-based shortfall cannot bound a batch, so
/// the count keeps every pump bounded.
const LOAD_BATCH_RECORDS: usize = 100;

/// Why the loader was woken. Reasons are advisory bookkeeping: the
/// pump always just restores the coverage invariant, whatever the
/// trigger — except ChannelSwitch and FilterChange, which clear the
/// buffer first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wakeup {
    ChannelSwitch,
    NearTop,
    Insert,
    FilterChange,
    RectChange,
}

impl Wakeup {
    fn bit(self) -> u8 {
        match self {
            Self::ChannelSwitch => 1 << 0,
            Self::NearTop => 1 << 1,
            Self::Insert => 1 << 2,
            Self::FilterChange => 1 << 3,
            Self::RectChange => 1 << 4,
        }
    }
}

/// Runtime message filter, applied in the load pipeline. Filtered-out
/// messages remain stored; they just never enter the buffer.
pub type FilterFn = Arc<SyncMutex<Box<dyn Fn(&MsgRecord) -> bool + Send>>>;

/// The loader state shared with the chatview. All kvdb access for this
/// chatview happens inside the pump task.
pub struct Loader {
    /// The bound channel's tree; None until the first `set_channel`.
    tree: SyncMutex<Option<(String, Tree)>>,
    /// Shared with the chatview and type nodes.
    buffer: Arc<AsyncMutex<MsgBuffer>>,
    filter: FilterFn,
    /// Wakers call wake(); the run loop drains the accumulated bits.
    cv: Arc<CondVar>,
    /// Wake reasons accumulated since the last pump took them, so
    /// wakes arriving while the pump runs are not lost.
    pending: AtomicU8,
    /// `(scroll, view_h)` as last seen by the chatview's draw path.
    viewport: SyncMutex<(f32, f32)>,
    /// Measures records during load (materializes type-node instances).
    measure: SyncMutex<Option<Arc<dyn Fn(&MsgRecord) -> f32 + Send + Sync>>>,
    /// Derives file messages from loaded privmsgs (fud URLs).
    derive: SyncMutex<Option<Arc<dyn Fn(&MsgRecord) -> Option<MsgRecord> + Send + Sync>>>,
    redraw: RedrawTrigger,
}

impl Loader {
    pub fn new(buffer: Arc<AsyncMutex<MsgBuffer>>, redraw: RedrawTrigger) -> Arc<Self> {
        Arc::new(Self {
            tree: SyncMutex::new(None),
            buffer,
            filter: Arc::new(SyncMutex::new(Box::new(|_| true))),
            cv: Arc::new(CondVar::new()),
            pending: AtomicU8::new(0),
            viewport: SyncMutex::new((0., 0.)),
            measure: SyncMutex::new(None),
            derive: SyncMutex::new(None),
            redraw,
        })
    }

    /// Wake the pump, recording the reason (bits coalesce).
    pub fn wake(&self, reason: Wakeup) {
        self.pending.fetch_or(reason.bit(), Ordering::SeqCst);
        self.cv.notify();
    }

    /// Take and clear the accumulated wake reasons.
    fn take_pending(&self) -> u8 {
        self.pending.swap(0, Ordering::SeqCst)
    }

    /// The chatview's draw/scroll path reports the current viewport.
    pub fn update_viewport(&self, scroll: f32, view_h: f32) {
        *self.viewport.lock() = (scroll, view_h);
    }

    /// Bind a channel tree and reload. Called from `set_channel`.
    pub fn bind(&self, name: String, tree: Tree) {
        t!("binding channel tree {name}");
        *self.tree.lock() = Some((name, tree));
        self.wake(Wakeup::ChannelSwitch);
    }

    /// Replace the runtime filter and rebuild the buffer through the
    /// load pipeline.
    pub fn set_filter(&self, f: Box<dyn Fn(&MsgRecord) -> bool + Send>) {
        *self.filter.lock() = f;
        self.wake(Wakeup::FilterChange);
    }

    /// Overwrite the stored entry for this composite key in place
    /// (confirmation rewrites the payload; never creates a duplicate).
    pub fn update(&self, ts: Timestamp, id: &MessageId, msg_type: MsgType, payload: &[u8]) {
        let tree = self.tree.lock();
        let Some((_, tree)) = tree.as_ref() else { return };
        let key = codec::encode_key(ts, id);
        let val = codec::encode_value(msg_type, payload);
        tree.insert(&key, &val).expect("cannot update chat entry");
    }

    /// Install the record-measuring callback the pump uses to give
    /// loaded records their heights (type nodes lay out text; the
    /// loader stays geometry-free).
    pub fn set_measure(&self, f: Arc<dyn Fn(&MsgRecord) -> f32 + Send + Sync>) {
        *self.measure.lock() = Some(f);
    }

    /// Install the derivation callback the pump uses to derive file
    /// messages from loaded privmsg text.
    pub fn set_derive(&self, f: Arc<dyn Fn(&MsgRecord) -> Option<MsgRecord> + Send + Sync>) {
        *self.derive.lock() = Some(f);
    }

    /// Persist a live message into the bound channel tree. Dedup by
    /// the composite key; returns false when the entry already exists.
    pub fn store(&self, ts: Timestamp, id: &MessageId, msg_type: MsgType, payload: &[u8]) -> bool {
        let tree = self.tree.lock();
        let Some((_, tree)) = tree.as_ref() else { return false };
        let key = codec::encode_key(ts, id);
        match tree.contains_key(&key) {
            Ok(true) => false,
            Err(_) => false,
            Ok(false) => {
                let val = codec::encode_value(msg_type, payload);
                tree.insert(&key, &val).expect("cannot persist chat entry");
                true
            }
        }
    }

    /// The background task: wait for wakes, drain the reason bits,
    /// pump. The reset-before-drain ordering makes wakes arriving
    /// during a pump observable on the next `wait`.
    pub async fn run(self: Arc<Self>) {
        loop {
            self.cv.wait().await;
            self.cv.reset();
            loop {
                let bits = self.take_pending();
                if bits == 0 {
                    break
                }
                self.pump(bits).await;
            }
        }
    }

    /// Restore the coverage invariant. `reasons` are the drained wake
    /// bits; ChannelSwitch/FilterChange reload the buffer from empty.
    async fn pump(&self, reasons: u8) {
        let reload = reasons & (Wakeup::ChannelSwitch.bit() | Wakeup::FilterChange.bit()) != 0;
        let (scroll, view_h) = *self.viewport.lock();

        // The viewport is only known once the draw path has evaluated
        // the rect (it reports (0, 0) before the first draw pass).
        // Loading before that would measure every layout at a nonsense
        // wrap width. The draw path wakes NearTop once real geometry
        // exists, and this pump runs then.
        if view_h <= 0. {
            t!("pump deferred: viewport not yet known");
            return
        }
        let margin = PRELOAD_MARGIN_FRAC * view_h;

        let mut buffer = self.buffer.lock().await;
        if reload {
            buffer.clear();
        }

        let covered = buffer.total_height();
        let need = scroll + view_h + margin;
        if !reload && covered >= need {
            // Advisory fast path: e.g. NearTop when already covered.
            return
        }
        let shortfall = need - covered;

        let tree_guard = self.tree.lock();
        let Some((tree_name, tree)) = tree_guard.as_ref() else { return };
        let tree_name = tree_name.clone();
        t!("pump reasons={reasons:#04x} tree={tree_name} covered={covered} need={need}");

        let filter = self.filter.lock();
        let mut batch = vec![];
        let mut batch_height = 0.;
        let touched_viewport = covered < scroll + view_h;

        // Iterate newest -> older, resuming below the oldest loaded
        // composite key.
        let iter = match buffer.oldest_ts() {
            Some(oldest) => {
                let key = codec::encode_key(oldest.saturating_sub(1), &MessageId([0xff; 32]));
                tree.range(..key).rev()
            }
            None => tree.iter().rev(),
        };

        for entry in iter {
            let (k, v) = entry.expect("kvdb iteration failed");
            let (ts, id) = codec::decode_key(&k);
            let mut rec = codec::decode_value(&v, ts, &id);
            if !filter(&rec) {
                continue
            }
            if let Some(measure) = self.measure.lock().as_ref() {
                rec.height = measure(&rec);
            }
            batch_height += rec.height;
            let rec_height = rec.height;
            let is_privmsg = rec.msg_type == MsgType::PrivMsg;
            batch.push(rec);
            if is_privmsg {
                if let Some(derive) = self.derive.lock().as_ref() {
                    if let Some(file_rec) = derive(batch.last().unwrap()) {
                        let mut file_rec = file_rec;
                        if let Some(measure) = self.measure.lock().as_ref() {
                            file_rec.height = measure(&file_rec);
                        }
                        batch_height += file_rec.height;
                        let _ = rec_height;
                        batch.push(file_rec);
                    }
                }
            }
            // Stop once the batch covers the shortfall (or, until
            // heights are measured, the count cap keeps us bounded).
            if batch_height >= shortfall || batch.len() >= LOAD_BATCH_RECORDS {
                break
            }
        }
        drop(filter);

        let inserted = buffer.insert_batch(batch);

        // Separators derived by the batch arrive unmeasured (they are
        // not in the tree); give them heights through the type nodes.
        if let Some(measure) = self.measure.lock().as_ref() {
            let mut ids = vec![];
            for rec in buffer.iter_display_order() {
                if rec.msg_type.is_derived() && rec.height == 0. {
                    ids.push(rec.id);
                }
            }
            for id in ids {
                if let Some(rec) = buffer.record(&id).cloned() {
                    let h = measure(&rec);
                    buffer.set_height_key(&(rec.ts, rec.id), h);
                }
            }
        }
        drop(buffer);
        drop(tree_guard);

        t!("pump loaded {inserted} records for tree {tree_name}");
        if inserted > 0 && touched_viewport {
            self.redraw.trigger();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::chatview::{codec, MessageId, MsgType};

    fn fixture_db(tag: &str, lines: &[(u64, u8)]) -> Tree {
        let path = std::env::temp_dir()
            .join(format!("darkfi-chatview-loader-{tag}-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let db = kvdb_overlay::Database::open_default(&path).unwrap();
        let tree = db.open_tree_default("chat").unwrap();
        for (ts, idb) in lines {
            let mut id = [0u8; 32];
            id[0] = *idb;
            let id = MessageId(id);
            let payload = codec::encode_privmsg_payload("nick", "text", true);
            let val = codec::encode_value(MsgType::PrivMsg, &payload);
            let key = codec::encode_key(*ts, &id);
            tree.insert(&key, &val).unwrap();
        }
        tree
    }

    /// A buffer with separator maintenance off (these tests model
    /// stored records only; the buffer's separator tests cover the
    /// derived-record invariant).
    fn raw_buffer() -> MsgBuffer {
        let mut buf = MsgBuffer::new();
        buf.disable_separators();
        buf
    }

    fn ids_loaded(buffer: &MsgBuffer) -> Vec<u8> {
        let mut ids = vec![];
        for rec in buffer.iter_display_order() {
            ids.push(rec.id.0[0]);
        }
        ids
    }

    /// The full id matching `fixture_db`'s first-byte ids.
    fn fid(b: u8) -> MessageId {
        let mut id = [0u8; 32];
        id[0] = b;
        MessageId(id)
    }

    #[test]
    fn pump_loads_newest_first_in_display_order() {
        let buffer = Arc::new(AsyncMutex::new(raw_buffer()));

        let (redraw, _rx) = RedrawTrigger::new();
        let loader = Loader::new(buffer.clone(), redraw);
        loader.update_viewport(0., 500.);
        loader.bind(
            "test".to_string(),
            fixture_db("order", &[(100, b'a'), (300, b'c'), (200, b'b')]),
        );

        smol::block_on(loader.pump(Wakeup::ChannelSwitch.bit()));

        let buffer = smol::block_on(buffer.lock());
        assert_eq!(ids_loaded(&buffer), vec![b'c', b'b', b'a']);
        assert_eq!(buffer.oldest_ts(), Some(100));
    }

    #[test]
    fn pump_resumes_below_oldest_and_dedups() {
        let buffer = Arc::new(AsyncMutex::new(raw_buffer()));

        let (redraw, _rx) = RedrawTrigger::new();
        let loader = Loader::new(buffer.clone(), redraw);
        loader.update_viewport(0., 500.);
        loader.bind(
            "test".to_string(),
            fixture_db("resume", &[(100, b'a'), (200, b'b'), (300, b'c'), (400, b'd')]),
        );

        smol::block_on(loader.pump(Wakeup::ChannelSwitch.bit()));
        {
            let buffer = smol::block_on(buffer.lock());
            assert_eq!(ids_loaded(&buffer), vec![b'd', b'c', b'b', b'a']);
        }

        // A second pump (e.g. NearTop) resumes from below the oldest
        // loaded key and finds nothing new — no duplicates, no reload.
        loader.update_viewport(10_000., 500.);
        smol::block_on(loader.pump(Wakeup::NearTop.bit()));
        let buffer = smol::block_on(buffer.lock());
        assert_eq!(ids_loaded(&buffer), vec![b'd', b'c', b'b', b'a']);
    }

    #[test]
    fn filter_change_reloads_without_filtered_records() {
        let buffer = Arc::new(AsyncMutex::new(raw_buffer()));

        let (redraw, _rx) = RedrawTrigger::new();
        let loader = Loader::new(buffer.clone(), redraw);
        loader.update_viewport(0., 500.);
        loader.bind(
            "test".to_string(),
            fixture_db("filter", &[(100, b'a'), (200, b'b'), (300, b'c')]),
        );

        loader.set_filter(Box::new(|rec| rec.id.0[0] != b'b'));

        smol::block_on(loader.pump(Wakeup::ChannelSwitch.bit() | Wakeup::FilterChange.bit()));
        let buffer = smol::block_on(buffer.lock());
        assert_eq!(ids_loaded(&buffer), vec![b'c', b'a']);
    }

    #[test]
    fn deleted_records_stay_gone_after_reload() {
        let buffer = Arc::new(AsyncMutex::new(raw_buffer()));

        let (redraw, _rx) = RedrawTrigger::new();
        let loader = Loader::new(buffer.clone(), redraw);
        loader.update_viewport(0., 500.);
        let tree = fixture_db("delete", &[(100, b'a'), (200, b'b'), (300, b'c')]);
        loader.bind("test".to_string(), tree.clone());

        smol::block_on(loader.pump(Wakeup::ChannelSwitch.bit()));

        // Remove from buffer and tree (what the chatview's delete_line
        // does), then force a full reload.
        {
            let mut buffer = smol::block_on(buffer.lock());
            assert!(buffer.remove(&fid(b'b')));
        }
        let key = codec::encode_key(200, &fid(b'b'));
        tree.remove(&key).unwrap();

        loader.set_filter(Box::new(|_| true));
        smol::block_on(loader.pump(Wakeup::FilterChange.bit()));
        let buffer = smol::block_on(buffer.lock());
        assert_eq!(ids_loaded(&buffer), vec![b'c', b'a']);
    }

    #[test]
    fn pump_derives_separators_for_loaded_days() {
        let buffer = Arc::new(AsyncMutex::new(MsgBuffer::new()));
        let (redraw, _rx) = RedrawTrigger::new();
        let loader = Loader::new(buffer.clone(), redraw);
        loader.update_viewport(0., 500.);

        // Two days of stored messages (2026-08-29/30, unix ms).
        use chrono::{Local, TimeZone};
        let ts = |day: i64, h: u32| {
            let date =
                chrono::NaiveDate::from_ymd_opt(2026, 8, 29).unwrap() + chrono::Duration::days(day);
            let dt = date.and_hms_opt(h, 0, 0).unwrap();
            Local.from_local_datetime(&dt).unwrap().timestamp_millis() as u64
        };
        loader.bind(
            "test".to_string(),
            fixture_db("seps", &[(ts(0, 10), b'a'), (ts(0, 11), b'b'), (ts(1, 9), b'c')]),
        );

        smol::block_on(loader.pump(Wakeup::ChannelSwitch.bit()));

        let buffer = smol::block_on(buffer.lock());
        let kinds: Vec<u8> = buffer
            .iter_display_order()
            .map(|r| if r.msg_type.is_derived() { b'|' } else { r.id.0[0] })
            .collect();
        assert_eq!(kinds, vec![b'c', b'|', b'b', b'a', b'|']);
    }

    #[test]
    fn corrupt_entries_panic_loudly() {
        let buffer = Arc::new(AsyncMutex::new(MsgBuffer::new()));
        let (redraw, _rx) = RedrawTrigger::new();
        let loader = Loader::new(buffer.clone(), redraw);
        loader.update_viewport(0., 500.);

        let path = std::env::temp_dir()
            .join(format!("darkfi-chatview-loader-corrupt-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let db = kvdb_overlay::Database::open_default(&path).unwrap();
        let tree = db.open_tree_default("chat").unwrap();
        // Old untagged format: must fail, never misread.
        let mut legacy = vec![];
        darkfi_serial::Encodable::encode(&"alice".to_string(), &mut legacy).unwrap();
        let key = codec::encode_key(100, &MessageId([b'a'; 32]));
        tree.insert(&key, &legacy).unwrap();
        loader.bind("test".to_string(), tree);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            smol::block_on(loader.pump(Wakeup::ChannelSwitch.bit()));
        }));
        assert!(result.is_err(), "corrupt entry must panic");
    }
}
