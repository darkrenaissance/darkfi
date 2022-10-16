// use std::any::Any;

use pasta_curves::pallas;

pub struct State {
    pub public_values: Vec<pallas::Base>,
}

impl State {
    // pub fn new() -> Box<dyn Any> {
    //     Box::new(Self { public_values: Vec::new() })
    // }

    pub fn add_public_value(&mut self, public_value: pallas::Base) {
        self.public_values.push(public_value)
    }

    // pub fn public_exists(&self, public_value: &pallas::Base) -> bool {
    //     self.public_values.iter().any(|v| v == public_value)
    // }
}
