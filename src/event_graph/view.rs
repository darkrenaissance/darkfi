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

use std::{collections::HashMap, sync::Arc};

use darkfi_serial::{Decodable, Encodable};
use smol::lock::Mutex;

use crate::{
    event_graph::{
        events_queue::EventsQueuePtr,
        model::{Event, EventId},
    },
    Result,
};

use super::EventMsg;

pub type ViewPtr<T> = Arc<Mutex<View<T>>>;

pub struct View<T: Send + Sync> {
    pub seen: HashMap<EventId, Event<T>>,
    pub events_queue: EventsQueuePtr<T>,
}

impl<T> View<T>
where
    T: Send + Sync + Encodable + Decodable + Clone + EventMsg,
{
    pub fn new(events_queue: EventsQueuePtr<T>) -> Self {
        Self { seen: HashMap::new(), events_queue }
    }

    pub async fn process(&mut self) -> Result<Event<T>> {
        // loop {
        let new_event = self.events_queue.fetch().await?;
        Ok(new_event)
        // }
    }
}
