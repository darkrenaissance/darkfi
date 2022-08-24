use std::any::Any;

use crate::example_contract::foo::validate::CallData;

use crate::demo::{/*CallDataBase, StateRegistry, ZkContractInfo, */ FuncCall, ZkContractTable,};

pub struct Builder {}

impl Builder {
    pub fn build(self, zk_bins: &ZkContractTable) -> FuncCall {
        let mut proofs = vec![];

        let call_data = CallData {};

        FuncCall {
            contract_id: "EXAMPLE".to_string(),
            func_id: "EXAMPLE::foo()".to_string(),
            call_data: Box::new(call_data),
            proofs,
        }
    }
}
