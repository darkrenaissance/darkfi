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

use std::any::{Any, TypeId};

use darkfi::crypto::types::DrkCircuitField;
use darkfi_sdk::crypto::PublicKey;
use darkfi_serial::{Encodable, SerialDecodable, SerialEncodable};

use crate::{
    contract::dao::{DaoBulla, State, CONTRACT_ID},
    util::{CallDataBase, StateRegistry, Transaction, UpdateBase},
};

pub fn state_transition(
    _states: &StateRegistry,
    func_call_index: usize,
    parent_tx: &Transaction,
) -> Result<Box<dyn UpdateBase + Send>> {
    let func_call = &parent_tx.func_calls[func_call_index];
    let call_data = func_call.call_data.as_any();

    assert_eq!((*call_data).type_id(), TypeId::of::<CallData>());
    let call_data = call_data.downcast_ref::<CallData>();

    // This will be inside wasm so unwrap is fine.
    let call_data = call_data.unwrap();

    Ok(Box::new(Update { dao_bulla: call_data.dao_bulla.clone() }))
}

#[derive(Clone)]
pub struct Update {
    pub dao_bulla: DaoBulla,
}

impl UpdateBase for Update {
    fn apply(self: Box<Self>, states: &mut StateRegistry) {
        // Lookup dao_contract state from registry
        let state = states.lookup_mut::<State>(*CONTRACT_ID).unwrap();
        // Add dao_bulla to state.dao_bullas
        state.add_dao_bulla(self.dao_bulla);
    }
}

// Custom program errors can be defined here 
#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {}

type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct CallData {
    pub dao_bulla: DaoBulla,
}

impl CallDataBase for CallData {
    fn zk_public_values(&self) -> Vec<(String, Vec<DrkCircuitField>)> {
        vec![("dao-mint".to_string(), vec![self.dao_bulla.0])]
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn signature_public_keys(&self) -> Vec<PublicKey> {
        //  no signatures involved in mint phase so empty
        vec![]
    }

    fn encode_bytes(
        &self,
        mut writer: &mut dyn std::io::Write,
    ) -> std::result::Result<usize, std::io::Error> {
        self.encode(&mut writer)
    }
}
