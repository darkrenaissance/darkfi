from vm import VariableType

def to_initial_caps(snake_str):
    components = snake_str.split("_")
    return "".join(x.title() for x in components)

def display(contract):
    indent = " " * 4

    print(r"""use super::vm::{ZKVirtualMachine, CryptoOperation, AllocType, ConstraintInstruction};

pub fn load_zkvm() -> ZKVirtualMachine {
    ZKVirtualMachine {
        alloc: vec![""")

    for symbol, variable in contract.alloc.items():
        print("%s // %s" % (indent * 3, symbol))

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

    for op in contract.ops:
        print("%s// %s" % (indent * 3, op.line))
        print("%sCryptoOperation::%s(%s)," % (
            indent * 3,
            to_initial_caps(op.command),
            ", ".join(str(index) for index in op.args)
        ))

    print("%s]," % (indent * 2))
    print("%sconstraints: vec![" % (indent * 2))

    for constraint in contract.constraints:
        args_part = ""
        if constraint.args:
            print("%s// %s" % (indent *3, constraint.args_comment()))
            args_part = ", ".join(str(index) for index in constraint.args)
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

