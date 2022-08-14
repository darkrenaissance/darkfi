use pasta_curves::pallas;
use std::any::{Any, TypeId};

use crate::{
    dao_contract::mint::CallData,
    demo::{StateRegistry, Transaction},
    Result,
};

#[derive(Clone)]
pub struct DaoBulla(pub pallas::Base);

/// This DAO state is for all DAOs on the network. There should only be a single instance.
pub struct State {
    dao_bullas: Vec<DaoBulla>,
}

impl State {
    pub fn new() -> Box<dyn Any> {
        Box::new(Self { dao_bullas: Vec::new() })
    }

    pub fn add_bulla(&mut self, bulla: DaoBulla) {
        self.dao_bullas.push(bulla);
    }
}
