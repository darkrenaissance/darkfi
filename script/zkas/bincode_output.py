import struct

def varuint(value):
    if value <= 0xfc:
        return struct.pack("<B", value)
    elif value <= 0xffff:
        return struct.pack("<BH", 0xfd, value)
    elif value <= 0xffffffff:
        return struct.pack("<BI", 0xfe, value)
    else:
        return struct.pack("<BQ", 0xff, value)

def write_len(output, objects):
    output.write(varuint(len(objects)))

def write_value(fmt, output, value):
    value_bytes = struct.pack("<" + fmt, value)
    output.write(value_bytes)

def write_u8(output, value):
    write_value("B", output, value)
def write_u32(output, value):
    write_value("I", output, value)

def output_contract(output, contract):
    write_len(output, contract.name)
    output.write(contract.name.encode())
    write_len(output, contract.witness)
    for type_id, variable, _ in contract.witness:
        write_len(output, variable)
        output.write(variable.encode())
        write_u8(output, type_id)
    write_len(output, contract.code)
    for code in contract.code:
        func_id = code.func_format.func_id
        write_u8(output, func_id)
        for arg_idx in code.arg_idxs:
            write_u32(output, arg_idx)

def output(output, contracts, constants):
    write_len(output, constants.variables())
    for variable in constants.variables():
        write_len(output, variable)
        output.write(variable.encode())
        type_id = constants.lookup(variable)
        write_u8(output, type_id)
    write_len(output, contracts)
    for contract in contracts:
        output_contract(output, contract)

