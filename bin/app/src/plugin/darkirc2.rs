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

use std::{
    io::Cursor,
    sync::{Arc, OnceLock, Weak},
};

use darkfi_serial::{Decodable, Encodable};

use crate::{
    error::Result,
    prop::{BatchGuardPtr, PropertyStr, Role},
    scene::{MethodCallSub, Pimpl, SceneNodePtr, SceneNodeWeak},
    ui::{
        chatview::{MessageId, Timestamp},
        OnModify,
    },
    ExecutorPtr,
};

macro_rules! t { ($($arg:tt)*) => { trace!(target: "plugin::darkirc2", $($arg)*); } }
macro_rules! d { ($($arg:tt)*) => { debug!(target: "plugin::darkirc2", $($arg)*); } }
macro_rules! i { ($($arg:tt)*) => { info!(target: "plugin::darkirc2", $($arg)*); } }
macro_rules! e { ($($arg:tt)*) => { error!(target: "plugin::darkirc2", $($arg)*); } }
macro_rules! w { ($($arg:tt)*) => { warn!(target: "plugin::darkirc2", $($arg)*); } }

pub type DarkIrc2Ptr = Arc<DarkIrc2>;

pub struct DarkIrc2 {
    node: SceneNodeWeak,
    tasks: OnceLock<Vec<smol::Task<()>>>,

    /// Configured nickname
    nick: PropertyStr,
}

impl DarkIrc2 {
    pub async fn new(node: SceneNodeWeak, sg_root: SceneNodePtr, ex: ExecutorPtr) -> Result<Pimpl> {
        i!("Starting DarkIRC backend (stub)");

        let node_ref = &node.upgrade().unwrap();
        let nick = PropertyStr::wrap(node_ref, Role::Internal, "nick", 0).unwrap();

        let self_ = Arc::new(Self { node: node.clone(), tasks: OnceLock::new(), nick });

        self_.clone().start(sg_root, ex).await;

        Ok(Pimpl::DarkIrc2(self_))
    }

    /// User wants to send a msg
    async fn handle_send(&self, timest: Timestamp, channel: String, msg: String) {
        t!("method called: send: {timest} {channel} {msg}");
    }

    /// User requested to reconnect
    async fn handle_reconnect(&self) {
        t!("method called: reconnect");
    }

    /// Send a notification about a new message
    pub async fn notify_recv(
        &self,
        channel: String,
        timestamp: Timestamp,
        id: MessageId,
        nick: String,
        msg: String,
    ) {
        let mut arg_data = vec![];
        channel.encode(&mut arg_data).unwrap();
        timestamp.encode(&mut arg_data).unwrap();
        id.encode(&mut arg_data).unwrap();
        nick.encode(&mut arg_data).unwrap();
        msg.encode(&mut arg_data).unwrap();

        let node = self.node.upgrade().unwrap();
        node.trigger("recv", arg_data).await.unwrap();
    }

    /// Send a notification when theres a change in number of peers or the DAG status
    pub async fn notify_connect(&self, peers_count: usize, is_dag_synced: bool) {
        let mut arg_data = vec![];
        (peers_count as u32).encode(&mut arg_data).unwrap();
        is_dag_synced.encode(&mut arg_data).unwrap();

        let node = self.node.upgrade().unwrap();
        node.trigger("connect", arg_data).await.unwrap();
    }

    async fn start(self: Arc<Self>, sg_root: SceneNodePtr, ex: ExecutorPtr) {
        let me = Arc::downgrade(&self);

        let node = &self.node.upgrade().unwrap();

        let method_sub = node.subscribe_method_call("send").unwrap();
        let me2 = me.clone();
        let send_method_task =
            ex.spawn(async move { while Self::process_send(&me2, &method_sub).await {} });

        let reconnect_method_sub = node.subscribe_method_call("reconnect").unwrap();
        let me2 = me.clone();
        let reconnect_method_task =
            ex.spawn(
                async move { while Self::process_reconnect(&me2, &reconnect_method_sub).await {} },
            );

        let mut on_modify = OnModify::new(ex.clone(), self.node.clone(), me.clone());
        async fn save_nick(self_: Arc<DarkIrc2>, _batch: BatchGuardPtr) {
            t!("nick changed: {}", self_.nick.get());
        }
        on_modify.when_change(self.nick.prop(), save_nick);

        let mut tasks = vec![send_method_task, reconnect_method_task];
        tasks.append(&mut on_modify.tasks);
        self.tasks.set(tasks).unwrap();
    }

    async fn process_send(me: &Weak<Self>, sub: &MethodCallSub) -> bool {
        let Ok(method_call) = sub.receive().await else {
            d!("Event relayer closed");
            return false
        };

        assert!(method_call.send_res.is_none());

        fn decode_data(data: &[u8]) -> std::io::Result<(Timestamp, String, String)> {
            let mut cur = Cursor::new(&data);
            let timest = Timestamp::decode(&mut cur).unwrap();
            let channel = String::decode(&mut cur)?;
            let msg = String::decode(&mut cur)?;
            Ok((timest, channel, msg))
        }

        let Ok((timest, channel, msg)) = decode_data(&method_call.data) else {
            e!("send() method invalid arg data");
            return true
        };

        let Some(self_) = me.upgrade() else {
            panic!("self destroyed before send task was stopped!");
        };

        self_.handle_send(timest, channel, msg).await;

        true
    }

    async fn process_reconnect(me: &Weak<Self>, sub: &MethodCallSub) -> bool {
        if sub.receive().await.is_err() {
            d!("Reconnect method closed");
            return false
        }

        let Some(self_) = me.upgrade() else {
            e!("DarkIrc destroyed before reconnect completed");
            return false
        };

        self_.handle_reconnect().await;

        true
    }
}
