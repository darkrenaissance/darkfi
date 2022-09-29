use fxhash::FxHashMap;

use darkfi::Result;

use crate::model::{Event, EventId, EventQueueArc, Model};

struct View {
    seen: FxHashMap<EventId, Event>,
}

impl View {
    pub fn new() -> Self {
        Self { seen: FxHashMap::default() }
    }

    pub async fn process(&mut self, event_queue: EventQueueArc) -> Result<()>  {
        loop {
            let new_event = event_queue.fetch().await?;
            // TODO sort the events
            self.seen.insert(new_event.hash(), new_event);
        }
    }
}
