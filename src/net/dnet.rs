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

use url::Url;

use super::channel::ChannelInfo;
use crate::util::time::NanoTimestamp;

macro_rules! dnetev {
    ($self:expr, $event_name:ident, $($code:tt)*) => {
        {
            if $self.p2p().dnet_enabled.load(std::sync::atomic::Ordering::SeqCst) {
                let event = DnetEvent::$event_name(dnet::$event_name $($code)*);
                $self.p2p().dnet_notify(event).await;
            }
        }
    };
}
pub(crate) use dnetev;

#[derive(Clone, Debug)]
pub struct MessageInfo {
    pub chan: ChannelInfo,
    pub cmd: String,
    pub time: NanoTimestamp,
}

// Needed by the dnetev!() macro
pub type SendMessage = MessageInfo;
pub type RecvMessage = MessageInfo;

#[derive(Clone, Debug)]
pub struct InboundInfo {
    pub addr: Url,
    pub channel_id: u32,
}

pub type InboundConnected = InboundInfo;
pub type InboundDisconnected = InboundInfo;

#[derive(Clone, Debug)]
pub struct OutboundSlotSleeping {
    pub slot: u32,
}

#[derive(Clone, Debug)]
pub struct OutboundSlotConnecting {
    pub slot: u32,
    pub addr: Url,
}

#[derive(Clone, Debug)]
pub struct OutboundSlotConnected {
    pub slot: u32,
    pub addr: Url,
    pub channel_id: u32,
}

#[derive(Clone, Debug)]
pub struct OutboundSlotDisconnected {
    pub slot: u32,
    pub err: String,
}

#[derive(Clone, Debug)]
pub struct OutboundPeerDiscovery {
    pub attempt: u32,
    pub state: &'static str,
}

#[derive(Clone, Debug)]
pub struct DirectConnecting {
    pub connect_addr: Url,
}

#[derive(Clone, Debug)]
pub struct DirectConnected {
    pub connect_addr: Url,
    pub addr: Url,
    pub channel_id: u32,
}

#[derive(Clone, Debug)]
pub struct DirectDisconnected {
    pub connect_addr: Url,
    pub err: String,
}

pub type DirectPeerDiscovery = OutboundPeerDiscovery;

#[derive(Clone, Debug)]
pub enum DnetEvent {
    SendMessage(MessageInfo),
    RecvMessage(MessageInfo),
    InboundConnected(InboundConnected),
    InboundDisconnected(InboundDisconnected),
    OutboundSlotSleeping(OutboundSlotSleeping),
    OutboundSlotConnecting(OutboundSlotConnecting),
    OutboundSlotConnected(OutboundSlotConnected),
    OutboundSlotDisconnected(OutboundSlotDisconnected),
    OutboundPeerDiscovery(OutboundPeerDiscovery),
    DirectConnecting(DirectConnecting),
    DirectConnected(DirectConnected),
    DirectDisconnected(DirectDisconnected),
    DirectPeerDiscovery(DirectPeerDiscovery),
}
