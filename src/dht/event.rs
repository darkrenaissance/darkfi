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

use std::fmt::Debug;

use crate::{dht::DhtNode, net::ChannelPtr, Result};

type K = blake3::Hash;

#[derive(Clone, Debug)]
pub enum DhtEvent<N: DhtNode, V: Clone + Debug> {
    BootstrapStarted,
    BootstrapCompleted,
    PingReceived { from: ChannelPtr, result: Result<K> },
    PingSent { to: ChannelPtr, result: Result<()> },
    ValueFound { key: K, value: V },
    NodesFound { key: K, nodes: Vec<N> },
    ValueLookupStarted { key: K },
    NodesLookupStarted { key: K },
    ValueLookupCompleted { key: K, nodes: Vec<N>, values: Vec<V> },
    NodesLookupCompleted { key: K, nodes: Vec<N> },
}

impl<N: DhtNode, V: Clone + Debug> DhtEvent<N, V> {
    pub fn key(&self) -> Option<&blake3::Hash> {
        match self {
            DhtEvent::BootstrapStarted => None,
            DhtEvent::BootstrapCompleted => None,
            DhtEvent::PingReceived { .. } => None,
            DhtEvent::PingSent { .. } => None,
            DhtEvent::ValueFound { key, .. } => Some(key),
            DhtEvent::NodesFound { key, .. } => Some(key),
            DhtEvent::ValueLookupStarted { key } => Some(key),
            DhtEvent::NodesLookupStarted { key } => Some(key),
            DhtEvent::ValueLookupCompleted { key, .. } => Some(key),
            DhtEvent::NodesLookupCompleted { key, .. } => Some(key),
        }
    }

    pub fn into_value(self) -> Option<V> {
        match self {
            DhtEvent::ValueFound { value, .. } => Some(value),
            _ => None,
        }
    }
}
