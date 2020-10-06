from vm import VariableType, VariableRefType

def to_initial_caps(snake_str):
    components = snake_str.split("_")
    return "".join(x.title() for x in components)

def display(contract):
    indent = " " * 4

    print(r"""use super::vm::{ZKVirtualMachine, CryptoOperation, AllocType, ConstraintInstruction, VariableRef};
use bls12_381::Scalar;

pub fn load_zkvm() -> ZKVirtualMachine {
    ZKVirtualMachine {
        constants: vec![""")

    constants = list(contract.constants.items())
    constants.sort(key=lambda obj: obj[1][0])
    constants = [(obj[0], obj[1][1]) for obj in constants]
    for symbol, value in constants:
        print("%s// %s" % (indent * 3, symbol))
        assert len(value) == 32*2
        chunk_str = lambda line, n: \
            [line[i:i + n] for i in range(0, len(line), n)]
        chunks = chunk_str(value, 2)
        # Reverse the endianness
        # We allow literal numbers but rust wants little endian
        chunks = chunks[::-1]
        print("%sScalar::from_bytes(&[" % (indent * 3))
        for i in range(0, 32, 4):
            print("%s0x%s, 0x%s, 0x%s, 0x%s," % (indent * 4,
                chunks[i], chunks[i + 1], chunks[i + 2], chunks[i + 3]))
        print("%s]).unwrap()," % (indent * 3))

    print("%s]," % (indent * 2))
    print("%salloc: vec![" % (indent * 2))

    for symbol, variable in contract.alloc.items():
        print("%s// %s" % (indent * 3, symbol))

        if variable.type.name == VariableType.PRIVATE.name:
            typestring = "Private"
        elif variable.type.name == VariableType.PUBLIC.name:
            typestring = "Public"
        else:
            assert False

        print("%s(AllocType::%s, %s)," % (indent * 3, typestring,
                                          variable.index))

    print("%s]," % (indent * 2))
    print("%sops: vec![" % (indent * 2))

    def var_ref_str(var_ref):
        if var_ref.type.name == VariableRefType.AUX.name:
            return "VariableRef::Aux(%s)" % var_ref.index
        elif var_ref.type.name == VariableRefType.LOCAL.name:
            return "VariableRef::Local(%s)" % var_ref.index
        else:
            assert False

    for op in contract.ops:
        print("%s// %s" % (indent * 3, op.line))
        args_part = ""
        if op.command == "load":
            assert len(op.args) == 2
            args_part = "(%s, %s)" % (var_ref_str(op.args[0]), op.args[1].index)
        elif op.args:
            args_part = ", ".join(var_ref_str(var_ref) for var_ref in op.args)
            args_part = "(%s)" % args_part
        print("%sCryptoOperation::%s%s," % (
            indent * 3,
            to_initial_caps(op.command),
            args_part
        ))

    print("%s]," % (indent * 2))
    print("%sconstraints: vec![" % (indent * 2))

    for constraint in contract.constraints:
        args_part = ""
        if constraint.args:
            print("%s// %s" % (indent *3, constraint.args_comment()))
            args = constraint.args[:]
            if (constraint.command == "lc0_add_coeff" or
                constraint.command == "lc1_add_coeff" or
                constraint.command == "lc2_add_coeff" or
                constraint.command == "lc0_add_one_coeff" or
                constraint.command == "lc1_add_one_coeff" or
                constraint.command == "lc2_add_one_coeff"):
                args[0] = args[0][0]
            args_part = ", ".join(str(index) for index in args)
            args_part = "(%s)" % args_part
        print("%sConstraintInstruction::%s%s," % (
            indent * 3,
            to_initial_caps(constraint.command),
            args_part
        ))
    print(r"""        ],
        aux: vec![],
        params: None,
        verifying_key: None,
    }
}""")

