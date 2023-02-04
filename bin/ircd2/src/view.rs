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

use async_std::sync::{Arc, Mutex};
use std::collections::HashMap;

use darkfi::Result;

pub type ViewPtr = Arc<Mutex<View>>;

use crate::{
    events_queue::EventsQueuePtr,
    model::{Event, EventId},
};

pub struct View {
    pub seen: HashMap<EventId, Event>,
    pub events_queue: EventsQueuePtr,
}

impl View {
    pub fn new(events_queue: EventsQueuePtr) -> Self {
        Self { seen: HashMap::new(), events_queue }
    }

    pub async fn process(&mut self) -> Result<Event> {
        // loop {
        let new_event = self.events_queue.fetch().await?;
        Ok(new_event)
        // }
    }
}
