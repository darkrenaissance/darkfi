from .types import type_id_to_name, func_id_to_name

def output(output, contracts, constants):
    output.write("Constants:\n")
    for variable in constants.variables():
        type_id = constants.lookup(variable)
        output.write(f"  {type_id} {variable}\n")
    for contract in contracts:
        output.write(f"{contract.name}:\n")

        output.write(f"  Witness:\n")
        for type_id, variable, _ in contract.witness:
            type_name = type_id_to_name[type_id]
            output.write(f"    {type_name} {variable}\n")

        output.write(f"  Code:\n")
        for code in contract.code:
            output.write(f"    # args = {code.args}\n")
            output.write(f"    {code.func_name()} {code.return_values} "
                             f"{code.arg_idxs}\n")

