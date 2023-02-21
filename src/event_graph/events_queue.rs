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

use async_std::sync::Arc;
use darkfi_serial::{Decodable, Encodable};

use crate::{event_graph::model::Event, Error, Result};

use super::EventMsg;

pub type EventsQueuePtr<T> = Arc<EventsQueue<T>>;

pub struct EventsQueue<T: Send + Sync>(
    smol::channel::Sender<Event<T>>,
    smol::channel::Receiver<Event<T>>,
);

impl<T> EventsQueue<T>
where
    T: Send + Sync + Encodable + Decodable + Clone + EventMsg,
{
    pub fn new() -> EventsQueuePtr<T> {
        let (sn, rv) = smol::channel::unbounded();
        Arc::new(Self(sn, rv))
    }

    pub async fn fetch(&self) -> Result<Event<T>> {
        self.1.recv().await.map_err(Error::from)
    }

    pub async fn dispatch(&self, event: &Event<T>) -> Result<()> {
        self.0.send(event.clone()).await.map_err(Error::from)
    }
}
