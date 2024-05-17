from . import serial

class Op:
    NULL = 0
    ADD = 1
    SUB = 2
    MUL = 3
    DIV = 4
    CONST_BOOL = 5
    CONST_UINT_32 = 6
    CONST_FLOAT_32 = 7
    CONST_STR = 8
    LOAD_VAR = 9
    MIN = 11
    MAX = 12
    IS_EQUAL = 13
    LESS_THAN = 14
    FLOAT32_TO_UINT32 = 15

    @staticmethod
    def from_str(op):
        match op:
            case "null":
                return Op.NULL
            case "+":
                return Op.ADD
            case "-":
                return Op.SUB
            case "*":
                return Op.MUL
            case "/":
                return Op.DIV
            case "bool":
                return Op.CONST_BOOL
            case "u32":
                return Op.CONST_UINT_32
            case "f32":
                return Op.CONST_FLOAT_32
            case "str":
                return Op.CONST_STR
            case "load":
                return Op.LOAD_VAR
            case "min":
                return Op.MIN
            case "max":
                return Op.MAX
            case "==":
                return Op.IS_EQUAL
            case "<":
                return Op.LESS_THAN
            case "as_u32":
                return Op.FLOAT32_TO_UINT32

def encode_expr(by, code):
    op, args = code[0], code[1:]
    op = Op.from_str(op)
    serial.write_u8(by, op)
    match op:
        case Op.CONST_BOOL:
            serial.write_u8(by, int(args[0]))
        case Op.CONST_UINT_32:
            serial.write_u32(by, args[0])
        case Op.CONST_FLOAT_32:
            serial.write_f32(by, args[0])
        case Op.CONST_STR:
            serial.encode_str(by, args[0])
        case Op.LOAD_VAR:
            serial.encode_str(by, args[0])
        case _:
            for arg in args:
                encode_expr(by, arg)

# python -m pydrk.expr
if __name__ == "__main__":
    code = ["+",
        ["u32", 5],
        ["/",
            ["load", "sw"],
            ["u32", 2]
        ]
    ]
    code_s = bytearray()
    encode_expr(code_s, code)
    assert code_s == bytearray(
        [1, 6, 5, 0, 0, 0, 4, 9, 2, 115, 119, 6, 2, 0, 0, 0]
    )

