/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

use darkfi_serial::Encodable;
use log::debug;
use parking_lot::Mutex as SyncMutex;
use std::{fs::File, io::Write, sync::OnceLock};

use super::{DebugTag, GfxBufferId, GfxDrawCall, GfxTextureId, Vertex};
use crate::EpochIndex;

macro_rules! d { ($($arg:tt)*) => { debug!(target: "gfx::trax", $($arg)*); } }

pub struct Trax {
    file: File,
    buf: Vec<u8>,
}

impl Trax {
    fn new() -> Self {
        let path = crate::android::get_external_storage_path().join("trax.dat");
        let file = File::create(path).unwrap();
        Self { file, buf: vec![] }
    }

    pub fn clear(&mut self) {
        d!("clear");
        self.file.set_len(0).unwrap();
    }

    pub fn put_dcs(&mut self, epoch: EpochIndex, timest: u64, dcs: &Vec<(u64, GfxDrawCall)>) {
        d!("put_dcs({epoch}, {timest}, {dcs:?})");
        0u8.encode(&mut self.buf).unwrap();
        epoch.encode(&mut self.buf).unwrap();
        timest.encode(&mut self.buf).unwrap();
        dcs.encode(&mut self.buf).unwrap();
    }

    pub fn put_tex(&mut self, epoch: EpochIndex, tex: GfxTextureId, tag: DebugTag) {
        d!("put_tex({epoch}, {tex}, {tag:?})");
        1u8.encode(&mut self.buf).unwrap();
        epoch.encode(&mut self.buf).unwrap();
        tex.encode(&mut self.buf).unwrap();
        tag.encode(&mut self.buf).unwrap();
    }
    pub fn put_verts(
        &mut self,
        epoch: EpochIndex,
        verts: Vec<Vertex>,
        buf: GfxBufferId,
        tag: DebugTag,
        buftype: u8,
    ) {
        d!("put_verts({epoch}, ..., {buf}, {tag:?}, {buftype})");
        2u8.encode(&mut self.buf).unwrap();
        epoch.encode(&mut self.buf).unwrap();
        verts.encode(&mut self.buf).unwrap();
        buf.encode(&mut self.buf).unwrap();
        tag.encode(&mut self.buf).unwrap();
        buftype.encode(&mut self.buf).unwrap();
    }
    pub fn put_idxs(
        &mut self,
        epoch: EpochIndex,
        idxs: Vec<u16>,
        buf: GfxBufferId,
        tag: DebugTag,
        buftype: u8,
    ) {
        d!("put_idxs({epoch}, ..., {buf}, {tag:?}, {buftype})");
        3u8.encode(&mut self.buf).unwrap();
        epoch.encode(&mut self.buf).unwrap();
        idxs.encode(&mut self.buf).unwrap();
        buf.encode(&mut self.buf).unwrap();
        tag.encode(&mut self.buf).unwrap();
        buftype.encode(&mut self.buf).unwrap();
    }

    pub fn put_stat(&mut self, code: u8) {
        d!("put_stat({code})");
        code.encode(&mut self.buf).unwrap();
    }

    pub fn del_tex(&mut self, epoch: EpochIndex, tex: GfxTextureId, tag: DebugTag) {
        d!("del_tex({epoch}, {tex}, {tag:?})");
        4u8.encode(&mut self.buf).unwrap();
        epoch.encode(&mut self.buf).unwrap();
        tex.encode(&mut self.buf).unwrap();
        tag.encode(&mut self.buf).unwrap();
    }
    pub fn del_buf(&mut self, epoch: EpochIndex, buf: GfxBufferId, tag: DebugTag, buftype: u8) {
        d!("del_buf({epoch}, {buf}, {tag:?}, {buftype})");
        5u8.encode(&mut self.buf).unwrap();
        epoch.encode(&mut self.buf).unwrap();
        buf.encode(&mut self.buf).unwrap();
        tag.encode(&mut self.buf).unwrap();
        buftype.encode(&mut self.buf).unwrap();
    }

    pub fn set_curr(&mut self, dc: u64) {
        d!("set_curr({dc})");
        6u8.encode(&mut self.buf).unwrap();
        dc.encode(&mut self.buf).unwrap();
        self.flush();
    }
    pub fn set_instr(&mut self, idx: usize) {
        d!("set_instr({idx})");
        7u8.encode(&mut self.buf).unwrap();
        idx.encode(&mut self.buf).unwrap();
        self.flush();
    }

    pub fn flush(&mut self) {
        d!("flush");
        let buf = std::mem::take(&mut self.buf);
        if buf.is_empty() {
            d!(" -> skipping flush");
            return
        }
        buf.encode(&mut self.file).unwrap();
    }
}

static TRAX: OnceLock<SyncMutex<Trax>> = OnceLock::new();

pub(super) fn get_trax() -> &'static SyncMutex<Trax> {
    TRAX.get_or_init(|| SyncMutex::new(Trax::new()))
}
