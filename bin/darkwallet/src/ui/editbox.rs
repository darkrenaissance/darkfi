use std::sync::{Arc, Mutex, Weak};

use crate::scene::Pimpl;

// First refactor the event system
// Each event should have its own unique pipe
// Advantages:
// - less overhead when publishing msgs to ppl who dont need them
// - more advanced locking of streams when widgets capture input
// also add capturing and make use of it with editbox.

pub type EditBoxPtr = Arc<EditBox>;

pub struct EditBox {
}

impl EditBox {
    pub async fn new(
    ) -> Pimpl {
        let self_ = Arc::new(Self {});

        Pimpl::EditBox(self_)
    }
}

