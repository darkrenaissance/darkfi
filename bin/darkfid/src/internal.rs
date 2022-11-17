/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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

use darkfi::{tx::Transaction, Result};

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
