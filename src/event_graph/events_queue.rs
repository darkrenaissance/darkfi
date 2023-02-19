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

use crate::{event_graph::model::Event, Error, Result};

pub type EventsQueuePtr = Arc<EventsQueue>;

pub struct EventsQueue(smol::channel::Sender<Event>, smol::channel::Receiver<Event>);

impl EventsQueue {
    pub fn new() -> EventsQueuePtr {
        let (sn, rv) = smol::channel::unbounded();
        Arc::new(Self(sn, rv))
    }

    pub async fn fetch(&self) -> Result<Event> {
        self.1.recv().await.map_err(Error::from)
    }

    pub async fn dispatch(&self, event: &Event) -> Result<()> {
        self.0.send(event.clone()).await.map_err(Error::from)
    }
}
