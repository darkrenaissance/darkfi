import struct
from compile import VariableType, VariableRefType

class Operation:

    def __init__(self, ident, args):
        self.ident = ident
        self.args = args

class ArgVarRef:

    def __init__(self, type, index):
        self.type = type
        self.index = index

    def bytes(self):
        return struct.pack("<BI", self.type, self.index)

class ArgVarIndex:

    def __init__(self, _, index):
        self.index = index

    def bytes(self):
        return struct.pack("<I", self.index)

class ArgString:

    def __init__(self, description, index):
        self.description = description
        self.index = index

ops_table = {
        "set": Operation(0, [ArgVarRef, ArgVarRef]),
        "mul": Operation(1, [ArgVarRef, ArgVarRef]),
        "add": Operation(2, [ArgVarRef, ArgVarRef]),
        "sub": Operation(3, [ArgVarRef, ArgVarRef]),
        "divide": Operation(4, [ArgVarRef, ArgVarRef]),
        "double": Operation(5, [ArgVarRef]),
        "square": Operation(6, [ArgVarRef]),
        "invert": Operation(7, [ArgVarRef]),
        "unpack_bits": Operation(8, [ArgVarRef, ArgVarRef, ArgVarRef]),
        "local": Operation(9, []),
        "load": Operation(10, [ArgVarRef, ArgVarIndex]),
        "debug": Operation(11, [ArgString, ArgVarRef]),
        "dump_alloc": Operation(12, []),
        "dump_local": Operation(13, []),
}

constraint_ident_map = {
    "lc0_add": 0,
    "lc1_add": 1,
    "lc2_add": 2,
    "lc0_sub": 3,
    "lc1_sub": 4,
    "lc2_sub": 5,
    "lc0_add_one": 6,
    "lc1_add_one": 7,
    "lc2_add_one": 8,
    "lc0_sub_one": 9,
    "lc1_sub_one": 10,
    "lc2_sub_one": 11,
    "lc0_add_coeff": 12,
    "lc1_add_coeff": 13,
    "lc2_add_coeff": 14,
    "lc0_add_one_coeff": 15,
    "lc1_add_one_coeff": 16,
    "lc2_add_one_coeff": 17,
    "enforce": 18,
    "lc_coeff_reset": 19,
    "lc_coeff_double": 20,
}

def varuint(value):
    if value <= 0xfc:
        return struct.pack("<B", value)
    elif value <= 0xffff:
        return struct.pack("<BH", 0xfd, value)
    elif value <= 0xffffffff:
        return struct.pack("<BI", 0xfe, value)
    else:
        return struct.pack("<BQ", 0xff, value)

def export(output, contract_name, contract):
    output.write(varuint(len(contract_name)))
    output.write(contract_name.encode())

    constants = list(contract.constants.items())
    constants.sort(key=lambda obj: obj[1][0])
    constants = [(obj[0], obj[1][1]) for obj in constants]

    # Constants
    output.write(varuint(len(constants)))
    for symbol, value in constants:
        print("Constant '%s' = %s" % (symbol, value))
        # Bellman uses little endian for Scalars from_bytes function
        const_bytes = bytearray.fromhex(value)[::-1]
        assert len(const_bytes) == 32
        output.write(const_bytes)

    # Alloc
    output.write(varuint(len(contract.alloc)))
    for symbol, variable in contract.alloc.items():
        print("Alloc '%s' = (%s, %s)" % (symbol, 
                                         variable.type.name, variable.index))
        if variable.type.name == VariableType.PRIVATE.name:
            typeval = 0
        elif variable.type.name == VariableType.PUBLIC.name:
            typeval = 1
        else:
            assert False
        alloc_bytes = struct.pack("<BI", typeval, variable.index)
        assert len(alloc_bytes) == 5
        output.write(alloc_bytes)

    # Ops
    output.write(varuint(len(contract.ops)))
    for op in contract.ops:
        op_form = ops_table[op.command]
        output.write(struct.pack("B", op_form.ident))

        if op.command == "debug":
            # Special case
            assert len(op.args) == 1
            line_str = str(op.line).encode()
            output.write(varuint(len(line_str)))
            output.write(line_str)

            op_arg = op.args[0]
            if op_arg.type.name == VariableRefType.AUX.name:
                arg_type = 0
            elif op_arg.type.name == VariableRefType.LOCAL.name:
                arg_type = 1
            arg = ArgVarRef(arg_type, op_arg.index)
            output.write(arg.bytes())
            continue

        assert len(op_form.args) == len(op.args)
        for arg_form, op_arg in zip(op_form.args, op.args):
            if op_arg.type.name == VariableRefType.AUX.name:
                arg_type = 0
            elif op_arg.type.name == VariableRefType.LOCAL.name:
                arg_type = 1
            arg = arg_form(arg_type, op_arg.index)
            output.write(arg.bytes())
        print("Operation", op.command,
              [(arg.type.name, arg.index) for arg in op.args])

    # Constraints
    output.write(varuint(len(contract.constraints)))
    for constraint in contract.constraints:
        args = constraint.args[:]
        if (constraint.command == "lc0_add_coeff" or
            constraint.command == "lc1_add_coeff" or
            constraint.command == "lc2_add_coeff" or
            constraint.command == "lc0_add_one_coeff" or
            constraint.command == "lc1_add_one_coeff" or
            constraint.command == "lc2_add_one_coeff"):
            args[0] = args[0][0]
        print("Constraint", constraint.command, args)
        enum_ident = constraint_ident_map[constraint.command]
        output.write(struct.pack("B", enum_ident))
        for arg in args:
            output.write(struct.pack("<I", arg))

    # Params Map
    param_alloc = [(symbol, variable) for (symbol, variable)
                   in contract.alloc.items() if variable.is_param]
    output.write(varuint(len(param_alloc)))
    for symbol, variable in param_alloc:
        assert variable.is_param
        print("Public '%s' = %s" % (symbol, variable.index))
        symbol = symbol.encode()
        output.write(varuint(len(symbol)))
        output.write(symbol)
        output.write(struct.pack("<I", variable.index))

