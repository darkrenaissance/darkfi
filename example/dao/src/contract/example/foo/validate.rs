/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

use darkfi_sdk::crypto::PublicKey;
use darkfi_serial::{Encodable, SerialDecodable, SerialEncodable};
use pasta_curves::pallas;

use darkfi::{crypto::types::DrkCircuitField, Error as DarkFiError};

use crate::{
    contract::example::{state::State, CONTRACT_ID},
    util::{CallDataBase, StateRegistry, Transaction, UpdateBase},
};

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("ValueExists")]
    ValueExists,
    #[error("DarkFi error: {0}")]
    DarkFiError(String),
}

impl From<DarkFiError> for Error {
    fn from(err: DarkFiError) -> Self {
        Self::DarkFiError(err.to_string())
    }
}

#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct CallData {
    pub public_value: pallas::Base,
    pub signature_public: PublicKey,
}

impl CallDataBase for CallData {
    fn zk_public_values(&self) -> Vec<(String, Vec<DrkCircuitField>)> {
        vec![("example-foo".to_string(), vec![self.public_value])]
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn signature_public_keys(&self) -> Vec<PublicKey> {
        vec![self.signature_public]
    }

    fn encode_bytes(
        &self,
        mut writer: &mut dyn std::io::Write,
    ) -> std::result::Result<usize, std::io::Error> {
        self.encode(&mut writer)
    }
}

pub fn state_transition(
    states: &StateRegistry,
    func_call_index: usize,
    parent_tx: &Transaction,
) -> Result<Box<dyn UpdateBase + Send>> {
    let func_call = &parent_tx.func_calls[func_call_index];
    let call_data = func_call.call_data.as_any();

    assert_eq!((*call_data).type_id(), TypeId::of::<CallData>());
    let call_data = call_data.downcast_ref::<CallData>();

    // This will be inside wasm so unwrap is fine.
    let call_data = call_data.unwrap();

    let example_state = states.lookup::<State>(*CONTRACT_ID).unwrap();

    if example_state.public_exists(&call_data.public_value) {
        return Err(Error::ValueExists)
    }

    Ok(Box::new(Update { public_value: call_data.public_value }))
}

#[derive(Clone)]
pub struct Update {
    public_value: pallas::Base,
}

impl UpdateBase for Update {
    fn apply(self: Box<Self>, states: &mut StateRegistry) {
        let example_state = states.lookup_mut::<State>(*CONTRACT_ID).unwrap();
        example_state.add_public_value(self.public_value);
    }
}
