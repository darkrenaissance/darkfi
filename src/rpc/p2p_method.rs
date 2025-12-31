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

use async_trait::async_trait;

use super::{
    jsonrpc::{JsonResponse, JsonResult},
    util::*,
};
use crate::net;

#[async_trait]
pub trait HandlerP2p: Sync + Send {
    async fn p2p_get_info(&self, id: u16, _params: JsonValue) -> JsonResult {
        let mut channels = Vec::new();
        for channel in self.p2p().hosts().channels() {
            let session = match channel.session_type_id() {
                net::session::SESSION_INBOUND => "inbound",
                net::session::SESSION_OUTBOUND => "outbound",
                net::session::SESSION_MANUAL => "manual",
                net::session::SESSION_REFINE => "refine",
                net::session::SESSION_SEED => "seed",
                net::session::SESSION_DIRECT => "direct",
                _ => panic!("invalid result from channel.session_type_id()"),
            };

            // For transport mixed connections send the mixed url to aid in debugging
            channels.push(json_map([
                ("url", JsonStr(channel.display_address().to_string())),
                ("session", json_str(session)),
                ("id", JsonNum(channel.info.id.into())),
            ]));
        }

        let mut slots = Vec::new();
        for channel_id in self.p2p().session_outbound().slot_info().await {
            slots.push(JsonNum(channel_id.into()));
        }

        let result =
            json_map([("channels", JsonArray(channels)), ("outbound_slots", JsonArray(slots))]);
        JsonResponse::new(result, id).into()
    }

    fn p2p(&self) -> net::P2pPtr;
}
