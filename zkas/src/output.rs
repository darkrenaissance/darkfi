use anyhow::Result;
use std::fs::File;
use std::io::{stdout, Write};
use varuint::*;

use crate::compiler::CompiledContract;
use crate::state::Constants;

pub fn text_output(contracts: Vec<CompiledContract>, constants: Constants) -> Result<()> {
    let mut f = stdout();
    f.write_all(b"Constants\n")?;

    for variable in constants.variables() {
        let type_id = constants.lookup(variable.to_string());
        f.write_all(format!("  {:#?} {}\n", type_id, variable).as_bytes())?;
    }

    for contract in contracts {
        f.write_all(format!("{}:\n", contract.name).as_bytes())?;

        f.write_all(b"  Witness:\n")?;
        for (type_id, variable, _) in contract.witness {
            f.write_all(format!("    {:#?} {}\n", type_id, variable).as_bytes())?;
        }

        f.write_all(b"  Code:\n")?;
        for code in contract.code {
            f.write_all(format!("    # args = {:?}\n", code.args).as_bytes())?;
            f.write_all(
                format!(
                    "    {:?} {:?} {:?}\n",
                    code.func_format.func_id, code.return_values, code.arg_idxs
                )
                .as_bytes(),
            )?;
        }
    }

    Ok(())
}

pub fn bincode_output(
    filename: &str,
    contracts: Vec<CompiledContract>,
    constants: Constants,
) -> Result<()> {
    //let mut cursor = Cursor::new(vec![]);
    let mut cursor = File::create(filename)?;

    cursor.write_varint(constants.variables().len() as u64)?;
    for variable in constants.variables() {
        cursor.write_varint(variable.len() as u64)?;
        let _ = cursor.write(variable.as_bytes())?;
        let type_id = constants.lookup(variable.to_string());
        let _ = cursor.write(&[type_id as u8])?;
    }

    cursor.write_varint(contracts.len() as u64)?;
    for contract in contracts {
        cursor.write_varint(contract.name.len() as u64)?;
        let _ = cursor.write(contract.name.as_bytes())?;

        cursor.write_varint(contract.witness.len() as u64)?;
        for (type_id, variable, _) in contract.witness {
            cursor.write_varint(variable.len() as u64)?;
            let _ = cursor.write(variable.as_bytes())?;
            let _ = cursor.write(&[type_id as u8])?;
        }

        cursor.write_varint(contract.code.len() as u64)?;
        for code in contract.code {
            let func_id = code.func_format.func_id;
            let _ = cursor.write(&[func_id as u8])?;

            for arg_idx in code.arg_idxs {
                let _ = cursor.write(&(arg_idx as u32).to_le_bytes())?;
            }
        }
    }

    //cursor.set_position(0);

    Ok(())
}
