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

use smol::{
    future::{Boxed, Future},
    lock::Mutex,
};
use tracing::debug;

use super::{
    super::{channel::ChannelPtr, p2p::P2pPtr, session::SessionBitFlag},
    protocol_base::ProtocolBasePtr,
};

type Constructor = Box<dyn Fn(ChannelPtr, P2pPtr) -> Boxed<ProtocolBasePtr> + Send + Sync>;

#[derive(Default)]
pub struct ProtocolRegistry {
    constructors: Mutex<Vec<(SessionBitFlag, Constructor)>>,
}

impl ProtocolRegistry {
    /// Instantiate a new [`ProtocolRegistry`]
    pub fn new() -> Self {
        Self::default()
    }

    /// `add_protocol()?`
    pub async fn register<C, F>(&self, session_flags: SessionBitFlag, constructor: C)
    where
        C: 'static + Fn(ChannelPtr, P2pPtr) -> F + Send + Sync,
        F: 'static + Future<Output = ProtocolBasePtr> + Send,
    {
        let constructor =
            move |channel, p2p| Box::pin(constructor(channel, p2p)) as Boxed<ProtocolBasePtr>;

        self.constructors.lock().await.push((session_flags, Box::new(constructor)));
    }

    pub async fn attach(
        &self,
        selector_id: SessionBitFlag,
        channel: ChannelPtr,
        p2p: P2pPtr,
    ) -> Vec<ProtocolBasePtr> {
        let mut protocols = vec![];

        for (session_flags, construct) in self.constructors.lock().await.iter() {
            // Skip protocols that are not registered for this session
            if selector_id & session_flags == 0 {
                debug!(
                    target: "net::protocol_registry",
                    "Skipping protocol attach [selector_id={selector_id:#b}, session_flags={session_flags:#b}]",
                );
                continue
            }

            let protocol = construct(channel.clone(), p2p.clone()).await;
            debug!(target: "net::protocol_registry", "Attached {}", protocol.name());
            protocols.push(protocol);
        }

        protocols
    }
}
