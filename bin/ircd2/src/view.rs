use fxhash::FxHashMap;

use darkfi::Result;

use crate::{
    events_queue::EventsQueuePtr,
    model::{Event, EventId},
};

pub struct View {
    seen: FxHashMap<EventId, Event>,
    events_queue: EventsQueuePtr,
}

impl View {
    pub fn new(events_queue: EventsQueuePtr) -> Self {
        Self { seen: FxHashMap::default(), events_queue }
    }

    pub async fn process(&mut self) -> Result<()> {
        loop {
            let new_event = self.events_queue.fetch().await?;
            // TODO sort the events
            self.seen.insert(new_event.hash(), new_event);
        }
    }
}
