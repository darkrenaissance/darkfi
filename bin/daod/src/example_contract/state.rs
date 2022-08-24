use std::any::Any;

pub struct State {}

impl State {
    pub fn new() -> Box<dyn Any> {
        Box::new(Self {})
    }
}
