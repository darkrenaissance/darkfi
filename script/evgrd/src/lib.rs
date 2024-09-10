/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use darkfi::event_graph::Event;
use darkfi_serial::{
    async_trait, deserialize_async, serialize_async, Encodable, SerialDecodable, SerialEncodable,
};
use std::sync::Arc;

pub const PROTOCOL_VERSION: u32 = 1;

pub struct LocalEventGraph {
    pub unref_tips: Vec<(u64, blake3::Hash)>,
}

impl LocalEventGraph {
    pub fn new() -> Self {
        Self { unref_tips: vec![] }
    }
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct VersionMessage {
    pub protocol_version: u32,
}

impl VersionMessage {
    pub fn new() -> Self {
        Self { protocol_version: PROTOCOL_VERSION }
    }
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FetchEventsMessage {
    unref_tips: Vec<(u64, blake3::Hash)>,
}

impl FetchEventsMessage {
    pub fn new(unref_tips: Vec<(u64, blake3::Hash)>) -> Self {
        Self { unref_tips }
    }
}

pub const MSG_EVENT: u8 = 1;
pub const MSG_FETCHEVENTS: u8 = 2;
