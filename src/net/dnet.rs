/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

use super::channel::ChannelInfo;
use crate::util::time::NanoTimestamp;
use url::Url;

macro_rules! dnet {
    ($self:expr, $($code:tt)*) => {
        {
            if *$self.p2p().dnet_enabled.lock().await {
                $($code)*
            }
        }
    };
}
pub(crate) use dnet;

#[derive(Clone, Debug)]
pub struct MessageInfo {
    pub chan: ChannelInfo,
    pub cmd: String,
    pub time: NanoTimestamp,
}

#[derive(Clone, Debug)]
pub struct OutboundConnect {
    pub slot: u32,
    pub addr: Url,
    pub channel_id: u32,
}

#[derive(Clone, Debug)]
pub enum DnetEvent {
    SendMessage(MessageInfo),
    RecvMessage(MessageInfo),
    //OutboundConnecting(OutboundConnect),
    OutboundConnected(OutboundConnect),
    OutboundDisconnected(u32),
}
