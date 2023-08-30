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
        for (url, channel) in self.p2p().channels().lock().await.iter() {
            let session = match channel.session_type_id() {
                net::session::SESSION_INBOUND => "inbound",
                net::session::SESSION_OUTBOUND => "outbound",
                net::session::SESSION_MANUAL => "manual",
                net::session::SESSION_SEED => "seed",
                _ => panic!("invalid result from channel.session_type_id()"),
            };
            channels.push(json_map([
                ("url", JsonStr(url.clone().into())),
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
