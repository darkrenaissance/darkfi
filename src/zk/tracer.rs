use std::{
    cell::RefCell,
    ops::{Deref, DerefMut},
};

use darkfi_sdk::{crypto::constants::OrchardFixedBases, pasta::pallas};
use halo2_gadgets::ecc as ecc_gadget;
use halo2_proofs::{arithmetic::Field, circuit::AssignedCell};

#[derive(Clone, Debug)]
pub enum DebugOpValue {
    EcPoint(pallas::Base, pallas::Base),
    Base(pallas::Base),
    Void,
}

#[derive(Clone)]
pub struct ZkTracer {
    pub opvalues: RefCell<Option<Vec<DebugOpValue>>>,
    init_allowed: bool,
}

impl ZkTracer {
    pub(crate) fn new(init_allowed: bool) -> Self {
        Self { opvalues: RefCell::new(None), init_allowed }
    }

    pub(crate) fn init(&self) {
        if !self.init_allowed {
            return
        }
        *self.opvalues.borrow_mut() = Some(Vec::new());
    }

    pub(crate) fn clear(&self) {
        if let Some(opvalues) = self.opvalues.borrow_mut().deref_mut() {
            opvalues.clear();
        }
    }

    pub(crate) fn push_ecpoint(
        &self,
        point: &ecc_gadget::Point<pallas::Affine, ecc_gadget::chip::EccChip<OrchardFixedBases>>,
    ) {
        if let Some(opvalues) = self.opvalues.borrow_mut().deref_mut() {
            let (mut x, mut y) = (pallas::Base::ZERO, pallas::Base::ZERO);
            point.inner().x().value().map(|rx| x = *rx);
            point.inner().y().value().map(|ry| y = *ry);
            opvalues.push(DebugOpValue::EcPoint(x, y));
        }
    }

    pub(crate) fn push_base(&self, value: &AssignedCell<pallas::Base, pallas::Base>) {
        if let Some(opvalues) = self.opvalues.borrow_mut().deref_mut() {
            let mut x = pallas::Base::ZERO;
            value.value().map(|rx| x = *rx);
            opvalues.push(DebugOpValue::Base(x));
        }
    }

    pub(crate) fn push_void(&self) {
        if let Some(opvalues) = self.opvalues.borrow_mut().deref_mut() {
            opvalues.push(DebugOpValue::Void);
        }
    }

    pub(crate) fn assert_correct(&self, opcodes_len: usize) {
        if let Some(opvalues) = self.opvalues.borrow().deref() {
            assert_eq!(opvalues.len(), opcodes_len);
        }
    }
}
