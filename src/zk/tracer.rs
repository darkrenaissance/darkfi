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
    is_enabled: bool,
}

impl ZkTracer {
    pub(crate) fn new(init_allowed: bool) -> Self {
        Self { opvalues: RefCell::new(None), init_allowed, is_enabled: false }
    }

    pub(crate) fn init(&mut self) {
        if !self.init_allowed {
            panic!("Cannot initialize tracer for verifier circuit!");
        }
        self.is_enabled = true;
        *self.opvalues.borrow_mut() = Some(Vec::new());
    }

    pub(crate) fn clear(&self) {
        if !self.is_enabled {
            return
        }

        self.opvalues.borrow_mut().as_mut().unwrap().clear();
    }

    fn push(&self, value: DebugOpValue) {
        let mut binding = self.opvalues.borrow_mut();
        let opvalues = binding.as_mut().unwrap();
        opvalues.push(value);
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

        let opvalues_len = self.opvalues.borrow().as_ref().map_or(0, |v| v.len());
        assert_eq!(opvalues_len, opcodes_len);
    }
}
