use fxhash::FxHashSet;

use crate::model::{EventId, Model};

struct View {
    seen: FxHashSet<EventId>,
}

impl View {
    pub fn new() -> Self {
        Self { seen: FxHashSet::default() }
    }

    fn process(_model: &Model) {
        // This does 2 passes:
        // 1. Walk down all chains and get unseen events
        // 2. Order those events according to timestamp
        // Then the events are replayed to the IRC client
    }
}
