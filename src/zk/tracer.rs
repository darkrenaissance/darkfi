/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use std::sync::{Arc, Mutex};

use darkfi_sdk::{crypto::constants::OrchardFixedBases, pasta::pallas};
use halo2_gadgets::ecc as ecc_gadget;
use halo2_proofs::{arithmetic::Field, circuit::AssignedCell};

#[derive(Clone, Debug)]
pub enum DebugOpValue {
    EcPoint(pallas::Base, pallas::Base),
    Base(pallas::Base),
    Void,
}

#[derive(Clone, Debug)]
pub struct ZkTracer {
    pub opvalues: Arc<Mutex<Option<Vec<DebugOpValue>>>>,
    init_allowed: bool,
    is_enabled: bool,
}

impl ZkTracer {
    pub(crate) fn new(init_allowed: bool) -> Self {
        Self { opvalues: Arc::new(Mutex::new(None)), init_allowed, is_enabled: false }
    }

    pub(crate) fn init(&mut self) {
        if !self.init_allowed {
            panic!("Cannot initialize tracer for verifier circuit!");
        }
        self.is_enabled = true;
        *self.opvalues.lock().unwrap() = Some(Vec::new());
    }

    pub(crate) fn clear(&self) {
        if !self.is_enabled {
            return
        }

        self.opvalues.lock().unwrap().as_mut().unwrap().clear();
    }

    fn push(&self, value: DebugOpValue) {
        self.opvalues.lock().unwrap().as_mut().unwrap().push(value);
    }

    pub(crate) fn push_ecpoint(
        &self,
        point: &ecc_gadget::Point<pallas::Affine, ecc_gadget::chip::EccChip<OrchardFixedBases>>,
    ) {
        if !self.is_enabled {
            return
        }

        let (mut x, mut y) = (pallas::Base::ZERO, pallas::Base::ZERO);
        point.inner().x().value().map(|rx| x = *rx);
        point.inner().y().value().map(|ry| y = *ry);
        self.push(DebugOpValue::EcPoint(x, y));
    }

    pub(crate) fn push_base(&self, value: &AssignedCell<pallas::Base, pallas::Base>) {
        if !self.is_enabled {
            return
        }

        let mut x = pallas::Base::ZERO;
        value.value().map(|rx| x = *rx);
        self.push(DebugOpValue::Base(x));
    }

    pub(crate) fn push_void(&self) {
        if !self.is_enabled {
            return
        }

        self.push(DebugOpValue::Void);
    }

    pub(crate) fn assert_correct(&self, opcodes_len: usize) {
        if !self.is_enabled {
            return
        }

        let opvalues_len = self.opvalues.lock().unwrap().as_ref().map_or(0, |v| v.len());
        assert_eq!(opvalues_len, opcodes_len);
    }
}
