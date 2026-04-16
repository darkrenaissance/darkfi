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

//! Debug Event Graph (DEG) types for inspecting protocol-level
//! message flow. When DEG is enabled on an [`EventGraph`] instance,
//! every sent/received P2P message produces a [`DegEvent`] that can
//! be observed through the DEG publisher.

use crate::util::time::NanoTimestamp;

/// Metadata attached to a DEG observation.
#[derive(Clone, Debug)]
pub struct MessageInfo {
    /// Human-readable context lines (addresses, event IDs, etc.)
    pub info: Vec<String>,
    /// The protocol command that was observed (e.g. "EventPut").
    pub cmd: String,
    /// Wall-clock time when the message was observed.
    pub time: NanoTimestamp,
}

/// A debug event emitted by the Event Graph protocol handlers.
#[derive(Clone, Debug)]
pub enum DegEvent {
    /// A message was sent to a peer.
    SendMessage(MessageInfo),
    /// A message was received from a peer.
    RecvMessage(MessageInfo),
}
