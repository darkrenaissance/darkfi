use lazy_static::lazy_static;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TypeId {
    Base,
    Scalar,
    EcPoint,
    EcFixedPoint,
    LastId,
}

#[derive(Debug, Clone)]
pub enum FuncId {
    PoseidonHash,
    Add,
    ConstrainInstance,
    EcMulShort,
    EcMul,
    EcAdd,
    EcGetX,
    EcGetY,
}

lazy_static! {
    pub static ref ALLOWED_TYPES: HashMap<&'static str, TypeId> = {
        let mut map = HashMap::new();

        map.insert("Base", TypeId::Base);
        map.insert("Scalar", TypeId::Scalar);
        map.insert("EcFixedPoint", TypeId::EcFixedPoint);

        map
    };
}

#[derive(Debug, Clone)]
pub struct FuncFormat {
    pub func_id: FuncId,
    pub return_type_ids: Vec<TypeId>,
    pub param_types: Vec<TypeId>,
}

impl FuncFormat {
    pub fn new(func_id: FuncId, return_type_ids: &[TypeId], param_types: &[TypeId]) -> Self {
        FuncFormat {
            func_id,
            return_type_ids: return_type_ids.to_vec(),
            param_types: param_types.to_vec(),
        }
    }

    pub fn total_arguments(&self) -> usize {
        self.return_type_ids.len() + self.param_types.len()
    }
}

lazy_static! {
    pub static ref FUNCTION_FORMATS: HashMap<&'static str, FuncFormat> = {
        let mut map = HashMap::new();

        map.insert(
            "poseidon_hash",
            FuncFormat::new(
                FuncId::PoseidonHash,
                &[TypeId::Base],
                &[TypeId::Base, TypeId::Base],
            ),
        );

        map.insert(
            "add",
            FuncFormat::new(FuncId::Add, &[TypeId::Base], &[TypeId::Base, TypeId::Base]),
        );

        map.insert(
            "constrain_instance",
            FuncFormat::new(FuncId::ConstrainInstance, &[], &[TypeId::Base]),
        );

        map.insert(
            "ec_mul_short",
            FuncFormat::new(
                FuncId::EcMulShort,
                &[TypeId::EcPoint],
                &[TypeId::Base, TypeId::EcFixedPoint],
            ),
        );

        map.insert(
            "ec_mul",
            FuncFormat::new(
                FuncId::EcMul,
                &[TypeId::EcPoint],
                &[TypeId::Scalar, TypeId::EcFixedPoint],
            ),
        );

        map.insert(
            "ec_add",
            FuncFormat::new(
                FuncId::EcAdd,
                &[TypeId::EcPoint],
                &[TypeId::EcPoint, TypeId::EcPoint],
            ),
        );

        map.insert(
            "ec_get_x",
            FuncFormat::new(FuncId::EcGetX, &[TypeId::Base], &[TypeId::EcPoint]),
        );

        map.insert(
            "ec_get_y",
            FuncFormat::new(FuncId::EcGetY, &[TypeId::Base], &[TypeId::EcPoint]),
        );

        map
    };
}
