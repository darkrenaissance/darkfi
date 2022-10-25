use fxhash::FxHashMap;

use darkfi::Result;

use crate::model::{Event, EventId, EventsQueueArc};

struct View {
    seen: FxHashMap<EventId, Event>,
}

impl View {
    pub fn new() -> Self {
        Self { seen: FxHashMap::default() }
    }

    pub async fn process(&mut self, events_queue: EventsQueueArc) -> Result<()> {
        loop {
            let new_event = events_queue.fetch().await?;
            // TODO sort the events
            self.seen.insert(new_event.hash(), new_event);
        }
    }
}
