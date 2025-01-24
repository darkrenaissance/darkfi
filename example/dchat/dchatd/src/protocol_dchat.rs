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

// ANCHOR: protocol_dchat
use async_trait::async_trait;
use darkfi::{net, Result};
use log::debug;
use smol::Executor;
use std::sync::Arc;

use crate::dchatmsg::{DchatMsg, DchatMsgsBuffer};

pub struct ProtocolDchat {
    jobsman: net::ProtocolJobsManagerPtr,
    msg_sub: net::MessageSubscription<DchatMsg>,
    msgs: DchatMsgsBuffer,
}
// ANCHOR_END: protocol_dchat

// ANCHOR: constructor
impl ProtocolDchat {
    pub async fn init(channel: net::ChannelPtr, msgs: DchatMsgsBuffer) -> net::ProtocolBasePtr {
        debug!(target: "dchat", "ProtocolDchat::init() [START]");
        let message_subsytem = channel.message_subsystem();
        message_subsytem.add_dispatch::<DchatMsg>().await;

        let msg_sub =
            channel.subscribe_msg::<DchatMsg>().await.expect("Missing DchatMsg dispatcher!");

        Arc::new(Self {
            jobsman: net::ProtocolJobsManager::new("ProtocolDchat", channel.clone()),
            msg_sub,
            msgs,
        })
    }
    // ANCHOR_END: constructor

    // ANCHOR: receive
    async fn handle_receive_msg(self: Arc<Self>) -> Result<()> {
        debug!(target: "dchat", "ProtocolDchat::handle_receive_msg() [START]");
        while let Ok(msg) = self.msg_sub.receive().await {
            let msg = (*msg).to_owned();
            self.msgs.lock().await.push(msg);
        }

        Ok(())
    }
    // ANCHOR_END: receive
}

#[async_trait]
impl net::ProtocolBase for ProtocolDchat {
    // ANCHOR: start
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "dchat", "ProtocolDchat::ProtocolBase::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_msg(), executor.clone()).await;
        debug!(target: "dchat", "ProtocolDchat::ProtocolBase::start() [STOP]");
        Ok(())
    }
    // ANCHOR_END: start

    fn name(&self) -> &'static str {
        "ProtocolDchat"
    }
}
