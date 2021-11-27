import argparse
import sys

from zkas.types import *

class CompileException(Exception):

    def __init__(self, error_message, line):
        super().__init__(error_message)
        self.error_message = error_message
        self.line = line

class Constants:

    def __init__(self):
        self.table = []
        self.map = {}

    def add(self, variable, type_id):
        idx = len(self.table)
        self.table.append(type_id)
        self.map[variable] = idx

    def lookup(self, variable):
        idx = self.map[variable]
        return self.table[idx]

    def variables(self):
        return self.map.keys()

class SyntaxStruct:

    def __init__(self):
        self.contracts = {}
        self.circuits = {}
        self.constants = Constants()

    def parse_contract(self, line, it):
        assert line.tokens[0] == "contract"
        if len(line.tokens) != 3 or line.tokens[2] != "{":
            raise CompileException("malformed contract opening", line)
        name = line.tokens[1]
        if name in self.contracts:
            raise CompileException(f"duplicate contract {name}", line)
        lines = []

        while True:
            try:
                line = next(it)
            except StopIteration:
                raise CompileException(
    f"premature end of file while parsing {name} contract", line)

            assert len(line.tokens) > 0
            if line.tokens[0] == "}":
                break

            lines.append(line)

        self.contracts[name] = lines

    def parse_circuit(self, line, it):
        assert line.tokens[0] == "circuit"
        if len(line.tokens) != 3 or line.tokens[2] != "{":
            raise CompileException("malformed circuit opening", line)
        name = line.tokens[1]
        if name in self.circuits:
            raise CompileException(f"duplicate contract {name}", line)
        lines = []

        while True:
            try:
                line = next(it)
            except StopIteration:
                raise CompileException(
    f"premature end of file while parsing {name} circuit", line)

            assert len(line.tokens) > 0
            if line.tokens[0] == "}":
                break

            lines.append(line)

        self.circuits[name] = lines

    def parse_constant(self, line):
        assert line.tokens[0] == "constant"
        if len(line.tokens) != 3:
            raise CompileException("malformed constant line", line)
        _, type_name, variable = line.tokens
        if type_name not in allowed_types:
            raise CompileException("unknown type '{type}'", line)
        type_id = allowed_types[type_name]
        self.constants.add(variable, type_id)

    def verify(self):
        self.static_checks()
        schema = self.format_data()
        self.trace_circuits(schema)
        return schema

    def static_checks(self):
        for name, lines in self.contracts.items():
            for line in lines:
                if len(line.tokens) != 2:
                    raise CompileException("incorrect number of tokens", line)
                type, variable = line.tokens
                if type not in allowed_types:
                    raise CompileException(
                        f"unknown type specifier for variable {variable}", line)

        for name, lines in self.circuits.items():
            for line in lines:
                assert len(line.tokens) > 0
                func_name, args = line.tokens[0], line.tokens[1:]
                if func_name not in function_formats:
                    raise CompileException(f"unknown function call {func_name}",
                                         line)
                func_format = function_formats[func_name]
                if len(args) != func_format.total_arguments():
                    raise CompileException(
        f"incorrect number of arguments for function call {func_name}", line)

        # Finally check there are matching circuits and contracts
        all_names = set(self.circuits.keys()) | set(self.contracts.keys())

        for name in all_names:
            if name not in self.contracts:
                raise CompileException(f"missing contract for {name}", None)
            if name not in self.circuits:
                raise CompileException(f"missing circuit for {name}", None)

    def format_data(self):
        schema = []
        for name, circuit in self.circuits.items():
            assert name in self.contracts
            contract = self.contracts[name]

            witness = []
            for line in contract:
                assert len(line.tokens) == 2
                type_name, variable = line.tokens
                assert type_name in allowed_types
                type_id = allowed_types[type_name]
                witness.append((type_id, variable, line))

            code = []
            for line in circuit:
                assert len(line.tokens) > 0
                func_name, args = line.tokens[0], line.tokens[1:]
                assert func_name in function_formats
                func_format = function_formats[func_name]
                assert len(args) == func_format.total_arguments()

                return_values = []
                if func_format.return_type_ids:
                    rv_len = len(func_format.return_type_ids)
                    return_values, args = args[:rv_len], args[rv_len:]

                func_id = func_format.func_id
                code.append((func_format, return_values, args, line))

            schema.append((name, witness, code))
        return schema

    def trace_circuits(self, schema):
        for name, witness, code in schema:
            tracer = DynamicTracer(name, witness, code, self.constants)
            tracer.execute()

class DynamicTracer:

    def __init__(self, name, contract_witness, circuit_code, constants):
        self.name = name
        self.witness = contract_witness
        self.code = circuit_code
        self.constants = constants

    def execute(self):
        stack = {}

        # Load constants
        for variable in self.constants.variables():
            stack[variable] = self.constants.lookup(variable)

        # Preload stack with our witness values
        for type_id, variable, line in self.witness:
            stack[variable] = type_id

        for i, (func_format, return_values, args, code_line) \
            in enumerate(self.code):

            assert len(args) == len(func_format.param_types)
            for variable, type_id in zip(args, func_format.param_types):
                if variable not in stack:
                    raise CompileException(
                        f"variable '{variable}' is not defined", code_line)

                stack_type_id = stack[variable]
                if stack_type_id != type_id:
                    type_name = type_id_to_name[type_id]
                    stack_type_name = type_id_to_name[stack_type_id]
                    raise CompileException(
    f"variable '{variable}' has incorrect type. "
    f"Found {type_name} but expected variable of "
    f"type {stack_type_name}", code_line)

                assert len(return_values) == len(func_format.return_type_ids)

                for return_variable, return_type_id \
                    in zip(return_values, func_format.return_type_ids):

                    # Note that later variables shadow earlier ones.
                    # We accept this.

                    stack[return_variable] = return_type_id

class CodeLine:

    def __init__(self, func_format, return_values, args, arg_idxs, code_line):
        self.func_format = func_format
        self.return_values = return_values
        self.args = args
        self.arg_idxs = arg_idxs
        self.code_line = code_line

    def func_name(self):
        return func_id_to_name[self.func_format.func_id]

class CompiledContract:

    def __init__(self, name, witness, code):
        self.name = name
        self.witness = witness
        self.code = code

class Compiler:

    def __init__(self, witness, uncompiled_code, constants):
        self.witness = witness
        self.uncompiled_code = uncompiled_code
        self.constants = constants

    def compile(self):
        code = []

        # Each unique type_id has its own stack
        stacks = [[] for i in range(TYPE_ID_LAST)]
        # Map from variable name to stacks above
        stack_vars = {}

        def alloc(variable, type_id):
            assert type_id <= len(stacks)
            idx = len(stacks[type_id])
            # Add variable to the stack for its type_id
            stacks[type_id].append(variable)
            # Create mapping from variable name
            stack_vars[variable] = (type_id, idx)

        # Load constants
        for variable in self.constants.variables():
            type_id = self.constants.lookup(variable)
            alloc(variable, type_id)

        # Preload stack with our witness values
        for type_id, variable, line in self.witness:
            alloc(variable, type_id)

        for i, (func_format, return_values, args, code_line) \
            in enumerate(self.uncompiled_code):

            assert len(args) == len(func_format.param_types)

            arg_idxs = []

            # Loop through all arguments
            for variable, type_id in zip(args, func_format.param_types):
                assert type_id <= len(stacks)
                assert variable in stack_vars
                # Find the index for the M by N matrix of our variable
                loc_type_id, loc_idx = stack_vars[variable]
                assert type_id == loc_type_id
                assert stacks[loc_type_id][loc_idx] == variable

                # This is the info to be serialized, not the variable names
                arg_idxs.append(loc_idx)

            assert len(return_values) == len(func_format.return_type_ids)

            for return_variable, return_type_id \
                in zip(return_values, func_format.return_type_ids):

                # Allocate returned values so they can be used by
                # subsequent function calls.
                alloc(return_variable, return_type_id)

            code.append(CodeLine(func_format, return_values, args,
                                 arg_idxs, code_line))

        return code

class Line:

    def __init__(self, tokens, original_line, number):
        self.tokens = tokens
        self.orig = original_line
        self.number = number

    def __repr__(self):
        return f"Line({self.number}: {str(self.tokens)})"

def load(src_file):
    source = []
    for i, original_line in enumerate(src_file):
        # Remove whitespace on both sides
        line = original_line.strip()
        # Strip out comments
        line = line.split("#")[0]
        # Split at whitespace
        line = line.split()
        if not line:
            continue
        line_number = i + 1
        source.append(Line(line, original_line, line_number))
    return source

def parse(source):
    syntax = SyntaxStruct()
    it = iter(source)
    while True:
        try:
            line = next(it)
        except StopIteration:
            break

        assert len(line.tokens) > 0
        if line.tokens[0] == "contract":
            syntax.parse_contract(line, it)
        elif line.tokens[0] == "circuit":
            syntax.parse_circuit(line, it)
        elif line.tokens[0] == "constant":
            syntax.parse_constant(line)
        elif line.tokens[0] == "}":
            raise CompileException("unmatched delimiter '}'", line)
    return syntax

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("SOURCE", help="ZK script to compile")
    parser.add_argument("--output", default=None, help="output file")
    group = parser.add_mutually_exclusive_group()
    group.add_argument('--display', action='store_true',
                       help="show the compiled code in human readable format")
    group.add_argument('--bincode', action='store_true',
                       help="output compiled code to zkvm supervisor")
    args = parser.parse_args()

    with open(args.SOURCE, "r") as src_file:
        source = load(src_file)
    try:
        syntax = parse(source)
        schema = syntax.verify()
        contracts = []
        for name, witness, uncompiled_code in schema:
            compiler = Compiler(witness, uncompiled_code, syntax.constants)
            code = compiler.compile()
            contracts.append(CompiledContract(name, witness, code))
        constants = syntax.constants
        if args.display:
            from zkas.text_output import output
            if args.output is None:
                output(sys.stdout, contracts, constants)
            else:
                with open(outpath, "w") as file:
                    output(file, contracts, constants)
        elif args.bincode:
            from zkas.bincode_output import output
            outpath = args.output
            if args.output is None:
                outpath = args.SOURCE + ".bin"
            with open(outpath, "wb") as file:
                output(file, contracts, constants)
        else:
            from zkas.text_output import output
            if args.output is None:
                output(sys.stdout, contracts, constants)
            else:
                with open(outpath, "w") as file:
                    output(file, contracts, constants)
    except CompileException as ex:
        print(f"Error: {ex.error_message}", file=sys.stderr)
        if ex.line is not None:
            print(f"Line {ex.line.number}: {ex.line.orig}", file=sys.stderr)
        #return -1
        raise
    return 0

if __name__ == "__main__":
    sys.exit(main())

# todo: think about extendable payment scheme which
# is like bitcoin soft forks
