import json
import os
import sys

import codegen

symbol_table = {
    "contract": 1,
    "param": 2,
    "start": 0,
    "end": 0,
}

types_map = {
    "U64": "u64",
    "Fr": "jubjub::Fr",
    "Point": "jubjub::SubgroupPoint",
    "Scalar": "bls12_381::Scalar",
    "Bool": "bool"
}

command_desc = {
    "witness": (
        ("EdwardsPoint",    True),
        ("Point",           False)
    ),
    "assert_not_small_order": (
        ("EdwardsPoint",    False),
    ),
    "fr_as_binary_le": (
        ("Vec<Boolean>",    True),
        ("Fr",              False)
    ),
    "ec_mul_const": (
        ("EdwardsPoint",    True),
        ("Vec<Boolean>",    False),
        ("FixedGenerator",  False)
    ),
    "ec_add": (
        ("EdwardsPoint",    True),
        ("EdwardsPoint",    False),
        ("EdwardsPoint",    False),
    ),
    "ec_repr": (
        ("Vec<Boolean>",    True),
        ("EdwardsPoint",    False),
    ),
    "emit_ec": (
        ("EdwardsPoint",    False),
    ),
    "alloc_binary": (
        ("Vec<Boolean>",    True),
    ),
    "binary_clone": (
        ("Vec<Boolean>",    True),
        ("Vec<Boolean>",    False),
    ),
    "binary_extend": (
        ("Vec<Boolean>",    False),
        ("Vec<Boolean>",    False),
    ),
    "static_assert_binary_size": (
        ("Vec<Boolean>",    False),
        ("INTEGER",         False),
    ),
    "blake2s": (
        ("Vec<Boolean>",    True),
        ("Vec<Boolean>",    False),
        ("BlakePersonalization", False),
    ),
    "emit_binary": (
        ("Vec<Boolean>",    False),
    ),
}

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

        if command in symbol_table:
            if symbol_table[command] != len(args):
                eprint("error: wrong number of args for command '%s'" % command)
                eprint(line)
                return None
        elif command in command_desc:
            if len(command_desc[command]) != len(args):
                eprint("error: wrong number of args for command '%s'" % command)
                eprint(line)
                return None
        else:
            eprint("error: missing symbol for command '%s'" % command)
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

class Contract:

    def __init__(self, name, params, program):
        self.name = name
        self.params = params
        self.program = program

    def _includes(self):
        return \
r"""use bellman::{
    gadgets::{
        boolean,
        boolean::{AllocatedBit, Boolean},
        multipack,
        blake2s,
    },
    groth16, Circuit, ConstraintSystem, SynthesisError,
};
use bls12_381::Bls12;
use ff::{PrimeField, Field};
use group::Curve;
use zcash_proofs::circuit::ecc;
"""

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

            if expected_type == "INTEGER":
                continue

            if is_param:
                actual_type = self.params[argname]
            elif argname in self.constants:
                actual_type = self.constants[argname]
            else:
                # Check the stack here
                if argname not in self.stack:
                    eprint("error: cannot find value '%s' on the stack!" %
                           argname)
                    eprint(line)
                    return False

                actual_type = self.stack[argname]

        return True

    def _check_args(self, command, args, line):
        assert command in command_desc
        type_list = command_desc[command]
        assert len(type_list) == len(args)

        for (expected_type, is_new_val), (arg, is_param) in zip(type_list, args):
            if is_param:
                continue
            if is_new_val:
                continue
            if arg in self.stack:
                continue
            if arg in self.constants:
                continue

            if expected_type == "INTEGER":
                continue

            eprint("error: cannot find '%s' in the stack" % arg)
            eprint(line)
            return False
        return True

    def _compile_line(self, command, args, line):
        if (args := self._preprocess_args(args, line)) is None:
            return None
        if not self.type_checking(command, args, line):
            return None

        if not self._check_args(command, args, line):
            return None

        self.modify_stack(command, args)

        args = [self.carg(arg) for arg in args]

        try:
            codegen_method = getattr(codegen, command)
        except AttributeError:
            eprint("error: missing command '%s' does not exist" % command)
            eprint(line)
            return None

        return codegen_method(line, *args)

    def carg(self, arg):
        argname, is_param = arg
        if is_param:
            return "self.%s" % argname
        if argname in self.rename_consts:
            return self.rename_consts[argname]
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

    def compile(self, constants, aux):
        self.constants = constants
        code = ""

        code += self._includes()

        self.rename_consts = {}
        if "constants" in aux:
            for const_name, value in aux["constants"].items():
                if "module_includes" not in value:
                    continue
                if "maps_to" not in value:
                    eprint("error: bad aux config '%s', missing maps_to" %
                           const_name)
                mapped_type = value["maps_to"]
                code += "use %s::%s;\n" % (value["module_includes"], mapped_type)

                self.rename_consts[const_name] = mapped_type

        code += "\n"

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
        code += "Ok(())\n"

        code += "    }\n"
        code += "}\n"

        return code

def process(contents, aux):
    contents = clean(contents)
    constants, segments = make_segments(contents)
    if (constants := build_constants_table(constants)) is None:
        return False

    codes = []
    for segment in segments:
        if (contract := extract(segment)) is None:
            return False
        if (code := contract.compile(constants, aux)) is None:
            return False
        codes.append(code)

    # Success! Output finished product.
    [print(code) for code in codes]

    return True

def main(argv):
    if len(argv) != 2:
        eprint("pism FILENAME")
        return -1

    src_filename = argv[1]

    basename, _ = os.path.splitext(src_filename)
    aux_filename = basename + ".aux"
    aux = json.loads(open(aux_filename).read())

    contents = open(src_filename).read()
    if not process(contents, aux):
        return -2

    return 0

if __name__ == "__main__":
    sys.exit(main(sys.argv))

