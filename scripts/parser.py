import lark
import pprint
import re
import sys

class LineDesc:

    def __init__(self, level, text, lineno):
        self.level = level
        self.text = text
        self.lineno = lineno
        assert self.text[0] != ' '

    def __repr__(self):
        return "<%s:'%s'>" % (self.level, self.text)

def clean_line(line):
    lead_spaces = len(line) - len(line.lstrip(" "))
    level = lead_spaces / 4
    # Remove leading spaces
    if line.strip(" ") == "":
        return None
    line = line.lstrip(" ")
    # Remove all comments
    line = re.sub('#.*$', '', line).strip()
    if not line:
        return None
    return level, line

def parse(text):
    lines = text.split("\n")

    linedescs = []

    # These are to join open parenthesis
    current_line = ""
    paren_level = 0

    for lineno, line in enumerate(lines):
        if (lineinfo := clean_line(line)) is None:
            continue
        level, line = lineinfo

        for c in line:
            if c == "(":
                paren_level += 1
            elif c == ")":
                paren_level -= 1

        #print(level, paren_level, current_line)

        if paren_level < 0:
            print("error: too many closing paren )", file=sys.stderr)
            print("line:", lineno)
            return

        if current_line:
            current_line += " " + line
        else:
            current_line = line

        if paren_level > 0:
            continue

        #print(level, current_line)

        ldesc = LineDesc(level, current_line, lineno)
        linedescs.append(ldesc)

        current_line = ""

    if paren_level > 0:
        print("error: missing closing paren )", file=sys.stderr)
        return None

    return linedescs

def section(linedescs):
    sections = []

    current_section = None
    for desc in linedescs:
        if desc.level == 0:
            if current_section:
                sections.append(current_section)
            current_section = [desc]
            continue

        current_section.append(desc)
    sections.append(current_section)

    return sections

def classify(sections):
    consts = []
    funcs = []
    contracts = []

    for section in sections:
        assert len(section)

        if section[0].text == "const:":
            consts.append(section)
        elif section[0].text.startswith("def"):
            funcs.append(section)
        elif section[0].text.startswith("contract"):
            contracts.append(section)

    return consts, funcs, contracts

def tokenize_const(text):
    parser = lark.Lark(r"""
        value_map: name ":" type_def

        name: NAME

        ?type_def:   point
                  | blake2s_personalization
                  | pedersen_personalization
                  | list

        point: "Point"

        blake2s_personalization: "Blake2sPersonalization"

        pedersen_personalization: "PedersenPersonalization"

        list: "list<" type_def ">"

        %import common.CNAME -> NAME
        %import common.WS
        %ignore WS
    """, start="value_map")
    return parser.parse(text)

class ConstTransformer(lark.Transformer):
    def name(self, name):
        return str(name[0])

    def point(self, _):
        return "Point"
    def blake2s_personalization(self, _):
        return "Blake2sPersonalization"
    def pedersen_personalization(self, _):
        return "PedersenPersonalization"
    value_map = tuple
    list = list

def read_consts(consts):
    consts_map = {}

    for subsection in consts:
        assert subsection[0].text == "const:"

        for ldesc in subsection[1:]:
            tree = tokenize_const(ldesc.text)
            tokens = ConstTransformer().transform(tree)
            #print(tokens)
            name, typedesc = tokens
            consts_map[name] = typedesc

    #pprint.pprint(consts_map)
    return consts_map

class FuncDefTransformer(lark.Transformer):
    def func_name(self, name):
        return str(name[0])

    def param(self, obj):
        return tuple(obj)

    def param_name(self, name):
        return str(name[0])

    def u64(self, _):
        return "U64"
    def scalar(self, _):
        return "Scalar"
    def point(self, _):
        return "Point"
    def binary(self, _):
        return "Binary"

    def type(self, obj):
        return obj[0]

    func_def = list
    params = list
    type_list = list

def parse_func_def(text):
    parser = lark.Lark(r"""
        func_def: "def" func_name "(" params+ ")" "->" type_list ":"

        func_name: NAME
        params: param ("," param)*

        type_list: type
                 | "(" type ("," type)* ")"

        param: param_name ":" type
        param_name: NAME

        type: u64 | scalar | point | binary

        u64: "U64"
        scalar: "Scalar"
        point: "Point"
        binary: "Binary"

        %import common.CNAME -> NAME
        %import common.WS
        %ignore WS
    """, start="func_def")
    tree = parser.parse(text)
    tokens = FuncDefTransformer().transform(tree)
    assert len(tokens) == 3
    return tokens

def compile_func_header(func_def):
    func_name, params, retvals = func_def
    #print("Function:", func_name)
    #print("Params:", params)
    #print("Return values:", retvals)
    #print()

    param_str = ""
    for param, type in params:
        if param_str:
            param_str += ", "
        param_str += param + ": "
        if type == "U64":
            param_str += "u64"
        elif type == "Scalar":
            param_str += "&jubjub::Fr"
        else:
            print("error: unsupported param type", file=sys.stderr)
            print("line:", line.text, "line:", line.lineno)
            return None

    converted_retvals = []
    for type in retvals:
        if type == "Binary":
            converted_retvals.append("boolean::Boolean")
        else:
            print("error: unsupported return type", file=sys.stderr)
            print("line:", line.text, "line:", line.lineno)
            return None
    retvals = converted_retvals

    if len(retvals) == 1:
        retstr = retvals[0]
    else:
        retstr = "(" + ", ".join(retvals) + ")"

    header = r"""
fn %s<CS>(
    mut cs: CS,
    %s
) -> Result<%s, SynthesisError>
where
    CS: ConstraintSystem<bls12_381::Scalar>,
{
""" % (func_name, param_str, retstr)
    return header

def as_expr(line, stack, consts, expr, code):
    var_from, type_to = expr.children
    if var_from not in stack:
        print("error: variable from not in stack frame:", var_from,
              file=sys.stderr)
        print("line:", line.text, "line:", line.lineno)
        return None

    type_from = stack[var_from]

    if type_from == "U64" and type_to == "Binary":
        code += "boolean::u64_into_boolean_vec_le(" + \
            "cs.namespace(|| \"" + line.text + "\"), " + var_from + \
            ")?;"
    elif type_from == "Scalar" and type_to == "Binary":
        code += "boolean::field_into_boolean_vec_le(" + \
            "cs.namespace(|| \"" + line.text + "\"), &" + var_from + \
            ")?;"
    else:
        print("error: unknown type conversion!", file=sys.stderr)
        print("line:", line.text, "line:", line.lineno)
        return None

    #print(var_from, type_from, type_to)
    return code, type_to

def mul_expr(line, stack, consts, expr, code):
    var_a, var_b = expr.children
    #print("MUL", var_a, var_b)

    if var_b not in consts:
        print("error: unknown base!", file=sys.stderr)
        print("line:", line.text, "line:", line.lineno)
        return None

    base_type = consts[var_b]
    if base_type != "Point":
        print("error: unknown base type!", file=sys.stderr)
        print("line:", line.text, "line:", line.lineno)
        return None

    code += "ecc::fixed_base_multiplication(" + \
        "cs.namespace(|| \"" + line.text + "\"), &" + var_b + \
        ", &" + var_a + ")?;"

    return code, base_type

def add_expr(line, stack, consts, expr, code):
    var_a, var_b = expr.children

    if var_a not in stack or var_b not in stack:
        print("error: missing stack item!", file=sys.stderr)
        print("line:", line.text, "line:", line.lineno)
        return None

    result_type = stack[var_a]
    if stack[var_b] != result_type:
        print("error: non matching items for addition!", file=sys.stderr)
        print("line:", line.text, "line:", line.lineno)
        return None

    code += var_a + ".add(cs.namespace(|| \"" + line.text \
        + "\"), &" + var_b + ")?;"

    return (code, result_type)

def compile_let(line, stack, consts, statement):
    is_mutable = False
    if statement[0] == "mut":
        is_mutable = True
        statement = statement[1:]
    variable_name, variable_type = statement[0], statement[1]
    expr = statement[2]
    #print("LET", is_mutable, variable_name, variable_type)
    #print("  ", expr)
    code = "let " + ("mut " if is_mutable else "") + variable_name + " = "

    if expr.data == "as_expr":
        ceval = as_expr(line, stack, consts, expr, code)
    elif expr.data == "mul_expr":
        ceval = mul_expr(line, stack, consts, expr, code)
    elif expr.data == "add_expr":
        ceval = add_expr(line, stack, consts, expr, code)

    if ceval is None:
        return None

    code, type_to = ceval

    if variable_type != type_to:
        print("error: sub expr does not evaluate to correct type",
              file=sys.stderr)
        print("line:", line.text, "line:", line.lineno)
        return None

    stack[variable_name] = variable_type
            
    return code

def interpret_func(func, consts):
    func_def = parse_func_def(func[0].text)

    header = compile_func_header(func_def)
    if header is None:
        return

    subroutine = header
    indent = " " * 4

    stack = dict(func_def[1])
    emitted_types = []
    for line in func[1:]:
        statement_type, statement = interpret_func_line(line.text, stack, consts)
        if statement_type == "let":
            code = compile_let(line, stack, consts, statement)
            if code is None:
                return
            subroutine += indent + code + "\n"

        elif statement_type == "return":
            for var in statement:
                if var not in stack:
                    print("error: missing variable in stack!", file=sys.stderr)
                    print("line:", line.text, "line:", line.lineno)
                    return None

            if len(statement) == 1:
                code = "Ok(" + statement[0] + ")"
            else:
                code = "Ok(" + ",".join(statement) + ")"
            subroutine += indent + code + "\n"

        elif statement_type == "emit":
            assert len(statement) == 1
            variable = statement[0]
            if variable not in stack:
                print("error: missing variable in stack!", file=sys.stderr)
                print("line:", line.text, "line:", line.lineno)
                return None

            variable_type = stack[variable]

            if variable_type == "Point":
                code = variable + ".inputize(cs.namespace(|| \"" + \
                    line.text + "\"))?;"
            else:
                print("error: unable to inputize type!", file=sys.stderr)
                print("line:", line.text, "line:", line.lineno)
                return None

            emitted_types.append(variable_type)
            subroutine += indent + code + "\n"

    subroutine += "}"
    print(subroutine)

class CodeLineTransformer(lark.Transformer):
    def variable_name(self, name):
        return str(name[0])

    def let_statement(self, obj):
        return ("let", obj)
    def return_statement(self, obj):
        return ("return", obj)
    def emit_statement(self, obj):
        return ("emit", obj)

    def point(self, _):
        return "Point"
    def scalar(self, _):
        return "Scalar"
    def binary(self, _):
        return "Binary"
    def u64(self, _):
        return "U64"

    def type(self, typename):
        return str(typename[0])

    def mutable(self, _):
        return "mut"

    statement = list

def interpret_func_line(text, stack, consts):
    parser = lark.Lark(r"""
        statement: let_statement
                 | return_statement
                 | emit_statement

        let_statement: "let" [mutable] variable_name ":" type "=" expr
        mutable: "mut"

        ?expr: as_expr
            | mul_expr
            | add_expr

        as_expr: variable_name "as" type
        mul_expr: variable_name "*" variable_name
        add_expr: variable_name "+" variable_name

        return_statement: "return" variable_name
                        | "return" variable_tuple
        variable_tuple: "(" variable_name ("," variable_name)* ")"

        emit_statement: "emit" variable_name

        variable_name: NAME
        type: u64 | scalar | point | binary

        u64: "U64"
        scalar: "Scalar"
        point: "Point"
        binary: "Binary"

        %import common.CNAME -> NAME
        %import common.WS
        %ignore WS
    """, start="statement")
    tree = parser.parse(text)
    tokens = CodeLineTransformer().transform(tree)[0]
    return tokens

def main(argv):
    if len(argv) == 1:
        print("error: missing proof file", file=sys.stderr)
        return -1
    filename = sys.argv[1]
    text = open(filename, "r").read()

    if (linedescs := parse(text)) is None:
        return -1

    sections = section(linedescs)

    consts, funcs, contracts = classify(sections)

    consts = read_consts(consts)

    for func in funcs:
        interpret_func(func, consts)

if __name__ == "__main__":
    main(sys.argv)

