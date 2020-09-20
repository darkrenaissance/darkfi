import sys

def eprint(*args):
    print(*args, file=sys.stderr)

class Line:

    def __init__(self, text, line_number):
        self.text = text
        self.orig = text
        self.lineno = line_number

        self.clean()

    def clean(self):
        # Remove the comments
        self.text = self.text.split("#", 1)[0]
        # Remove whitespace
        self.text = self.text.strip()

    def is_empty(self):
        return bool(self.text)

    def __repr__(self):
        return "Line %s: %s" % (self.lineno, self.orig)

    def command(self):
        if not self.is_empty():
            return None
        return self.text.split(" ")[0]

    def args(self):
        if not self.is_empty():
            return None
        return self.text.split(" ")[1:]

def clean(contents):
    # Split input into lines
    contents = contents.split("\n")
    contents = [Line(line, i) for i, line in enumerate(contents)]
    # Remove empty blank lines
    contents = [line for line in contents if line.is_empty()]
    return contents

def make_segments(contents):
    constants = [line for line in contents if line.command() == "constant"]

    segments = []
    current_segment = []
    for line in contents:
        if line.command() == "contract":
            current_segment = []

        current_segment.append(line)

        if line.command() == "end":
            segments.append(current_segment)
            current_segment = []

    return constants, segments

def build_constants_table(constants):
    table = {}
    for line in constants:
        args = line.args()
        if len(args) != 2:
            eprint("error: wrong number of args")
            eprint(line)
            return None
        name, type = args
        table[name] = type
    return table

symbol_table = {
    "contract": 1,
    "param": 2,
    "start": 0,
    "end": 0,

    "witness": 2,
}

def extract(segment):
    assert segment
    # Does it have a declaration?
    if not segment[0].command() == "contract":
        eprint("error: missing contract declaration")
        eprint(segment[0])
        return None
    # Does it have an end?
    if not segment[-1].command() == "end":
        eprint("error: missing contract end")
        eprint(segment[-1])
        return None
    # Does it have a start?
    if not [line for line in segment if line.command() == "start"]:
        eprint("error: missing contract start")
        eprint(segment[0])
        return None

    for line in segment:
        command, args = line.command(), line.args()
        if symbol_table[command] != len(args):
            eprint("error: wrong number of args for command '%s'" % command)
            eprint(line)
            return None

    contract_name = segment[0].args()[0]

    start_index = [index for index, line in enumerate(segment)
                   if line.command() == "start"]
    if len(start_index) > 1:
        eprint("error: multiple start statements in contract '%s'" %
               contract_name)
        for index in start_index:
            eprint(segment[index])
        eprint("Aborting.")
        return None
    assert len(start_index) == 1
    start_index = start_index[0]

    header = segment[1:start_index]
    code = segment[start_index + 1:-1]

    params = {}
    for param_decl in header:
        args = param_decl.args()
        assert len(args) == 2
        name, type = args
        params[name] = type

    program = []
    for line in code:
        command, args = line.command(), line.args()
        program.append((command, args, line))

    return Contract(contract_name, params, program)

def to_initial_caps(snake_str):
    components = snake_str.split("_")
    return "".join(x.title() for x in components)

types_map = {
    "U64": "u64",
    "Fr": "jubjub::Fr",
    "Point": "jubjub::SubgroupPoint",
    "Scalar": "bls12_381::Scalar",
    "Bool": "bool"
}

command_desc = {
    "witness": (("EdwardsPoint", True), ("Point", False))
}

class Contract:

    def __init__(self, name, params, program):
        self.name = name
        self.params = params
        self.program = program

    def _compile_header(self):
        code = "pub struct %s {\n" % to_initial_caps(self.name)
        for param_name, param_type in self.params.items():
            try:
                mapped_type = types_map[param_type]
            except KeyError:
                return None
            code += "    pub %s: Option<%s>,\n" % (param_name, mapped_type)
        code += "}\n"
        return code

    def _compile_body(self):
        self.stack = {}
        code = "\n"
        #indent = " " * 8
        for command, args, line in self.program:
            if (code_text := self._compile_line(command, args, line)) is None:
                return None
            code += code_text + "\n"
        return code

    def _preprocess_args(self, args, line):
        nargs = []
        for arg in args:
            if not arg.startswith("param:"):
                nargs.append((arg, False))
                continue
            _, argname = arg.split(":", 1)
            if argname not in self.params:
                eprint("error: non-existant param referenced")
                eprint(line)
                return None
            nargs.append((argname, True))
        return nargs

    def type_checking(self, command, args, line):
        assert command in command_desc
        type_list = command_desc[command]
        if len(type_list) != len(args):
            eprint("error: wrong number of arguments!")
            eprint(line)
            return False

        for (expected_type, new_val), (argname, is_param) in \
            zip(type_list, args):
            # Only type check input arguments, not output values
            if new_val:
                continue

            if is_param:
                actual_type = self.params[argname]
            else:
                # Check the stack here
                if argname not in self.stack:
                    eprint("error: cannot find value '%s' on the stack!" %
                           argname)
                    eprint(line)
                    return False

                actual_type = self.stack[argname]

            return True

    def _compile_line(self, command, args, line):
        if (args := self._preprocess_args(args, line)) is None:
            return None
        if not self.type_checking(command, args, line):
            return None

        self.modify_stack(command, args)

        args = [self.carg(arg) for arg in args]

        if command == "witness":
            out, point = args
            return \
r"""let %s = ecc::EdwardsPoint::witness(
    cs.namespace(|| "%s"),
    %s.map(jubjub::ExtendedPoint::from))?;""" % (out, line, point)

    def carg(self, arg):
        argname, is_param = arg
        if is_param:
            return "self.%s" % argname
        return argname

    def modify_stack(self, command, args):
        type_list = command_desc[command]
        assert len(type_list) == len(args)
        for (expected_type, new_val), (argname, is_param) in \
            zip(type_list, args):
            if is_param:
                assert not new_val
                continue

            # Now apply the new values to the stack
            if new_val:
                self.stack[argname] = expected_type

    def compile(self):
        code = ""

        if (header := self._compile_header()) is None:
            return None
        code += header

        code += \
r"""impl Circuit<bls12_381::Scalar> for %s {
    fn synthesize<CS: ConstraintSystem<bls12_381::Scalar>>(
        self,
        cs: &mut CS,
    ) -> Result<(), SynthesisError> {
""" % to_initial_caps(self.name)

        if (body := self._compile_body()) is None:
            return None
        code += body

        code += "}\n"

        return code

def process(contents):
    contents = clean(contents)
    constants, segments = make_segments(contents)
    if (constants := build_constants_table(constants)) is None:
        return False

    codes = []
    for segment in segments:
        contract = extract(segment)
        if (code := contract.compile()) is None:
            return False
        codes.append(code)

    # Success! Output finished product.
    [print(code) for code in codes]

    return True

def main(argv):
    if len(argv) != 2:
        eprint("pism FILENAME")
        return -1

    contents = open(argv[1]).read()
    if not process(contents):
        return -2

    return 0

if __name__ == "__main__":
    sys.exit(main(sys.argv))

