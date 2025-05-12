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

use super::util::*;
use crate::net;

#[cfg(feature = "event-graph")]
use crate::event_graph;

#[cfg(feature = "net")]
impl From<net::channel::ChannelInfo> for JsonValue {
    fn from(info: net::channel::ChannelInfo) -> JsonValue {
        json_map([
            ("addr", JsonStr(info.resolve_addr.unwrap_or(info.connect_addr).to_string())),
            ("id", JsonNum(info.id.into())),
        ])
    }
}

#[cfg(feature = "net")]
impl From<net::dnet::MessageInfo> for JsonValue {
    fn from(info: net::dnet::MessageInfo) -> JsonValue {
        json_map([
            ("chan", info.chan.into()),
            ("cmd", JsonStr(info.cmd)),
            ("time", JsonStr(info.time.0.to_string())),
        ])
    }
}

#[cfg(feature = "net")]
impl From<net::dnet::InboundInfo> for JsonValue {
    fn from(info: net::dnet::InboundInfo) -> JsonValue {
        json_map([
            ("addr", JsonStr(info.addr.to_string())),
            ("channel_id", JsonNum(info.channel_id.into())),
        ])
    }
}

#[cfg(feature = "net")]
impl From<net::dnet::OutboundSlotSleeping> for JsonValue {
    fn from(info: net::dnet::OutboundSlotSleeping) -> JsonValue {
        json_map([("slot", JsonNum(info.slot.into()))])
    }
}

#[cfg(feature = "net")]
impl From<net::dnet::OutboundSlotConnecting> for JsonValue {
    fn from(info: net::dnet::OutboundSlotConnecting) -> JsonValue {
        json_map([("slot", JsonNum(info.slot.into())), ("addr", JsonStr(info.addr.to_string()))])
    }
}

#[cfg(feature = "net")]
impl From<net::dnet::OutboundSlotConnected> for JsonValue {
    fn from(info: net::dnet::OutboundSlotConnected) -> JsonValue {
        json_map([
            ("slot", JsonNum(info.slot.into())),
            ("addr", JsonStr(info.addr.to_string())),
            ("channel_id", JsonNum(info.channel_id.into())),
        ])
    }
}

#[cfg(feature = "net")]
impl From<net::dnet::OutboundSlotDisconnected> for JsonValue {
    fn from(info: net::dnet::OutboundSlotDisconnected) -> JsonValue {
        json_map([("slot", JsonNum(info.slot.into())), ("err", JsonStr(info.err))])
    }
}

#[cfg(feature = "net")]
impl From<net::dnet::OutboundPeerDiscovery> for JsonValue {
    fn from(info: net::dnet::OutboundPeerDiscovery) -> JsonValue {
        json_map([
            ("attempt", JsonNum(info.attempt.into())),
            ("state", JsonStr(info.state.to_string())),
        ])
    }
}

#[cfg(feature = "net")]
impl From<net::dnet::DirectConnecting> for JsonValue {
    fn from(info: net::dnet::DirectConnecting) -> JsonValue {
        json_map([("connect_addr", JsonStr(info.connect_addr.to_string()))])
    }
}

#[cfg(feature = "net")]
impl From<net::dnet::DirectConnected> for JsonValue {
    fn from(info: net::dnet::DirectConnected) -> JsonValue {
        json_map([
            ("connect_addr", JsonStr(info.connect_addr.to_string())),
            ("addr", JsonStr(info.addr.to_string())),
            ("channel_id", JsonNum(info.channel_id.into())),
        ])
    }
}

#[cfg(feature = "net")]
impl From<net::dnet::DirectDisconnected> for JsonValue {
    fn from(info: net::dnet::DirectDisconnected) -> JsonValue {
        json_map([
            ("connect_addr", JsonStr(info.connect_addr.to_string())),
            ("err", JsonStr(info.err)),
        ])
    }
}

#[cfg(feature = "net")]
impl From<net::dnet::DnetEvent> for JsonValue {
    fn from(event: net::dnet::DnetEvent) -> JsonValue {
        match event {
            net::dnet::DnetEvent::SendMessage(info) => {
                json_map([("event", json_str("send")), ("info", info.into())])
            }
            net::dnet::DnetEvent::RecvMessage(info) => {
                json_map([("event", json_str("recv")), ("info", info.into())])
            }
            net::dnet::DnetEvent::InboundConnected(info) => {
                json_map([("event", json_str("inbound_connected")), ("info", info.into())])
            }
            net::dnet::DnetEvent::InboundDisconnected(info) => {
                json_map([("event", json_str("inbound_disconnected")), ("info", info.into())])
            }
            net::dnet::DnetEvent::OutboundSlotSleeping(info) => {
                json_map([("event", json_str("outbound_slot_sleeping")), ("info", info.into())])
            }
            net::dnet::DnetEvent::OutboundSlotConnecting(info) => {
                json_map([("event", json_str("outbound_slot_connecting")), ("info", info.into())])
            }
            net::dnet::DnetEvent::OutboundSlotConnected(info) => {
                json_map([("event", json_str("outbound_slot_connected")), ("info", info.into())])
            }
            net::dnet::DnetEvent::OutboundSlotDisconnected(info) => {
                json_map([("event", json_str("outbound_slot_disconnected")), ("info", info.into())])
            }
            net::dnet::DnetEvent::OutboundPeerDiscovery(info) => {
                json_map([("event", json_str("outbound_peer_discovery")), ("info", info.into())])
            }
            net::dnet::DnetEvent::DirectConnecting(info) => {
                json_map([("event", json_str("direct_connecting")), ("info", info.into())])
            }
            net::dnet::DnetEvent::DirectConnected(info) => {
                json_map([("event", json_str("direct_connected")), ("info", info.into())])
            }
            net::dnet::DnetEvent::DirectDisconnected(info) => {
                json_map([("event", json_str("direct_disconnected")), ("info", info.into())])
            }
            net::dnet::DnetEvent::DirectPeerDiscovery(info) => {
                json_map([("event", json_str("direct_peer_discovery")), ("info", info.into())])
            }
        }
    }
}

#[cfg(feature = "event-graph")]
impl From<event_graph::Event> for JsonValue {
    fn from(event: event_graph::Event) -> JsonValue {
        let parents =
            event.header.parents.into_iter().map(|id| JsonStr(id.to_string())).collect::<Vec<_>>();
        json_map([
            ("timestamp", JsonNum(event.header.timestamp as f64)),
            ("content", JsonStr(bs58::encode(event.content()).into_string())),
            ("parents", JsonArray(parents)),
            ("layer", JsonNum(event.header.layer as f64)),
        ])
    }
}

#[cfg(feature = "event-graph")]
impl From<event_graph::deg::MessageInfo> for JsonValue {
    fn from(info: event_graph::deg::MessageInfo) -> JsonValue {
        json_map([
            ("info", JsonArray(info.info.into_iter().map(JsonStr).collect())),
            ("cmd", JsonStr(info.cmd)),
            ("time", JsonStr(info.time.0.to_string())),
        ])
    }
}

#[cfg(feature = "event-graph")]
impl From<event_graph::deg::DegEvent> for JsonValue {
    fn from(event: event_graph::deg::DegEvent) -> JsonValue {
        match event {
            event_graph::deg::DegEvent::SendMessage(info) => {
                json_map([("event", json_str("send")), ("info", info.into())])
            }
            event_graph::deg::DegEvent::RecvMessage(info) => {
                json_map([("event", json_str("recv")), ("info", info.into())])
            }
        }
    }
}
