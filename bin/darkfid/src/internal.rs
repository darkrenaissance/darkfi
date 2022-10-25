use darkfi::{
    consensus::ValidatorState,
    node::{state::StateUpdate, MemoryState},
    tx::Transaction,
    Result,
};

use super::Darkfid;

impl Darkfid {
    /// Apply a new `MemoryState` from the current validator state and simulate a state
    /// transition with the given `Transaction`. Returns a vec of `StateUpdate` on success.
    pub async fn simulate_transaction(&self, tx: &Transaction) -> Result<Vec<StateUpdate>> {
        // Grab the current state and apply a new MemoryState
        let validator_state = self.validator_state.read().await;
        let state = validator_state.state_machine.lock().await;
        let mem_state = MemoryState::new(state.clone());
        drop(state);
        drop(validator_state);

        ValidatorState::validate_state_transitions(mem_state, &[tx.clone()])
    }
}
