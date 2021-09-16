import argparse
import sys
from enum import Enum

alloc_commands = {
    "param": 1,
    "private": 1,
    "public": 1,
}

op_commands = {
    "set": 2,
    "mul": 2,
    "add": 2, 
    "sub": 2,
    "divide": 2,
    "double": 1,
    "square": 1,
    "invert": 1,
    "unpack_bits": 3,
    "load": 2,
    "local": 1,
    "debug": 1,
    "dump_alloc": 0,
    "dump_local": 0,
}

constraint_commands = {
    "lc0_add": 1,
    "lc1_add": 1,
    "lc2_add": 1,
    "lc0_sub": 1,
    "lc1_sub": 1,
    "lc2_sub": 1,
    "lc0_add_one": 0,
    "lc1_add_one": 0,
    "lc2_add_one": 0,
    "lc0_sub_one": 0,
    "lc1_sub_one": 0,
    "lc2_sub_one": 0,
    "lc0_add_coeff": 2,
    "lc1_add_coeff": 2,
    "lc2_add_coeff": 2,
    "lc0_add_constant": 1,
    "lc1_add_constant": 1,
    "lc2_add_constant": 1,
    "enforce": 0,
    "lc_coeff_reset": 0,
    "lc_coeff_double": 0,
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
        return "Line %s: %s" % (self.lineno, self.orig.lstrip())

    def command(self):
        if not self.is_empty():
            return None
        return self.text.split(" ")[0]

    def args(self):
        if not self.is_empty():
            return None
        return self.text.split()[1:]

def clean(contents):
    # Split input into lines
    contents = contents.split("\n")
    contents = [Line(line, i + 1) for i, line in enumerate(contents)]
    # Remove empty blank lines
    contents = [line for line in contents if line.is_empty()]
    return contents

def divide_sections(contents):
    state = "NOSCOPE"
    segments = {}
    current_segment = []
    contract_name = None

    for line in contents:
        if line.command() == "contract":
            if len(line.args()) != 1:
                eprint("error: missing contract name")
                eprint(line)
                return None
            contract_name = line.args()[0]

            if state == "NOSCOPE":
                assert not current_segment
                state = "INSCOPE"
                continue
            else:
                assert state == "INSCOPE"
                eprint("error: double contract entry violation")
                eprint(line)
                return None
        elif line.command() == "end":
            if len(line.args()) != 0:
                eprint("error: end takes no args")
                eprint(line)
                return None

            if state == "NOSCOPE":
                eprint("error: missing contract start for end")
                eprint(line)
                return None
            else:
                assert state == "INSCOPE"
                state = "NOSCOPE"
                segments[contract_name] = current_segment
                current_segment = []
                continue
        elif state == "NOSCOPE":
            # Ignore lines outside any contract
            continue

        current_segment.append(line)

    if state != "NOSCOPE":
        eprint("error: reached end of file with unclosed scope")
        return None

    return segments

def extract_relevant_lines(contract, commands_table):
    relevant_lines = []

    for line in contract:
        command = line.command()

        if command not in commands_table.keys():
            continue

        define = commands_table[command]

        if len(line.args()) != define:
            eprint("error: wrong number of args")
            return None

        relevant_lines.append(line)

    return relevant_lines

class VariableType(Enum):
    PUBLIC = 1
    PRIVATE = 2

class Variable:

    def __init__(self, symbol, index, type, is_param):
        self.symbol = symbol
        self.index = index
        self.type = type
        self.is_param = is_param

    def __repr__(self):
        return "<Variable %s:%s>" % (self.symbol, self.index)

def generate_alloc_table(contract):
    relevant_lines = extract_relevant_lines(contract, alloc_commands)
    alloc_table = {}
    for i, line in enumerate(relevant_lines):
        assert len(line.args()) == 1
        symbol = line.args()[0]

        command = line.command()

        if command == "param":
            type = VariableType.PRIVATE
            is_param = True
        elif command == "private":
            type = VariableType.PRIVATE
            is_param = False
        elif command == "public":
            type = VariableType.PUBLIC
            is_param = False
        else:
            assert False

        if symbol in alloc_table:
            eprint("error: duplicate symbol '%s'" % symbol)
            eprint(line)
            return None

        alloc_table[symbol] = Variable(symbol, i, type, is_param)

    return alloc_table

class Operation:

    def __init__(self, line, indexes):
        self.command = line.command()
        self.args = indexes
        self.line = line

class VariableRefType(Enum):
    AUX = 1
    LOCAL = 2
    CONST = 3

class VariableRef:

    def __init__(self, type, index):
        self.type = type
        self.index = index

    def __repr__(self):
        return "%s(%s)" % (self.type.name, self.index)

def symbols_list_to_refs(line, alloc, local_vars, constants):
    indexes = []
    for symbol in line.args():
        if symbol in alloc:
            # Lookup variable index
            index = alloc[symbol].index
            index = VariableRef(VariableRefType.AUX, index)
        elif symbol in local_vars:
            index = local_vars[symbol]
            index = VariableRef(VariableRefType.LOCAL, index)
        elif symbol in constants:
            index = constants[symbol][0]
            index = VariableRef(VariableRefType.CONST, index)
        else:
            eprint("error: missing unallocated symbol '%s'" % symbol)
            eprint(line)
            return None
        indexes.append(index)
    return indexes

def generate_ops_table(contract, alloc, constants):
    relevant_lines = extract_relevant_lines(contract, op_commands)
    ops = []
    local_vars = {}
    for line in relevant_lines:
        # This is a special case which creates a new local stack value
        if line.command() == "local":
            assert len(line.args()) == 1
            symbol = line.args()[0]
            local_vars[symbol] = len(local_vars)
            indexes = []
        else:
            if (indexes := symbols_list_to_refs(line, alloc, 
                                                local_vars, constants)) is None:
                return None

            # Handle this here directly since only the
            # load command deals with constants
            if line.command() == "load":
                assert len(indexes) == 2
                # This is the only command which uses consts
                if indexes[1].type != VariableRefType.CONST:
                    eprint("error: load command takes a const argument")
                    eprint(line)
                    return None
            elif any(index.type == VariableRefType.CONST for index in indexes):
                eprint("error: invalid const arg")
                eprint(line)
                return None

        ops.append(Operation(line, indexes))
    return ops

class Constraint:

    def __init__(self, line, lcargs):
        self.command = line.command()
        self.args = lcargs
        self.line = line

    def args_comment(self):
        return ", ".join("%s" % symbol for symbol in self.line.args())

def symbols_list_to_lcargs(line, alloc, constants):
    lcargs = []
    for symbol in line.args():
        if symbol in alloc:
            # Lookup variable index
            index = alloc[symbol].index
            lcargs.append(index)
        elif symbol in constants:
            value = constants[symbol]
            lcargs.append(value)
        else:
            eprint("error: missing unallocated symbol '%s'" % symbol)
            eprint(line)
            return None
    return lcargs

def generate_constraints_table(contract, alloc, constants):
    relevant_lines = extract_relevant_lines(contract, constraint_commands)
    constraints = []
    for line in relevant_lines:
        if (lcargs := symbols_list_to_lcargs(line, alloc, constants)) is None:
            return None
        constraints.append(Constraint(line, lcargs))
    return constraints

class Contract:

    def __init__(self, constants, alloc, ops, constraints):
        self.constants = constants
        self.alloc = alloc
        self.ops = ops
        self.constraints = constraints

    def __repr__(self):
        repr_str = ""

        repr_str += "Constants:\n"
        for symbol, value in self.constants.items():
            repr_str += "    // %s\n" % symbol
            repr_str += "    %s: %s\n" % value

        repr_str += "Alloc table:\n"
        for symbol, variable in self.alloc.items():
            repr_str += "    // %s\n" % symbol
            repr_str += "    %s %s\n" % (variable.type, variable.index)

        repr_str += "Operations:\n"
        for op in self.ops:
            repr_str += "    // %s\n" % op.line
            repr_str += "    %s %s\n" % (op.command, op.args)

        repr_str += "Constraints:\n"
        for constraint in self.constraints:
            if constraint.args:
                repr_str += "    // %s\n" % constraint.args_comment()
            repr_str += "    %s %s\n" % (constraint.command, constraint.args)

        repr_str += "Stats:\n"
        repr_str += "    Constants: %s\n" % len(self.constants)
        repr_str += "    Alloc: %s\n" % len(self.alloc)
        repr_str += "    Operations: %s\n" % len(self.ops)
        repr_str += "    Constraint Instructions: %s\n" % len(self.constraints)

        return repr_str

def compile(contract, constants):
    # Allocation table
    # symbol: Private/Public, is_param, index
    if (alloc := generate_alloc_table(contract)) is None:
        return None
    # Operations lines list
    if (ops := generate_ops_table(contract, alloc, constants)) is None:
        return None
    # Constraint commands
    if (constraints := generate_constraints_table(
            contract, alloc, constants)) is None:
        return None
    return Contract(constants, alloc, ops, constraints)

def parse_constants(contents):
    relevant_lines = [line for line in contents if line.command() == "constant"]
    constants = {}
    for line in relevant_lines:
        assert line.command() == "constant"
        if len(line.args()) != 2:
            eprint("error: wrong number of args for constant")
            eprint(line)
            return None
        symbol, value = line.args()

        try:
            int(value, 16)
        except ValueError:
            eprint("error: invalid constant value for '%s'" % symbol)
            eprint(line)
            return None

        if len(value) != 32*2 + 2 or value[:2] != "0x":
            eprint("error: invalid hex value for constant")
            eprint(line)
            return None

        # Remove 0x prefix
        value = value[2:]

        constants[symbol] = (len(constants), value)
    return constants

def process(contents):
    # Remove left whitespace
    contents = clean(contents)
    # Parse all constants
    if (constants := parse_constants(contents)) is None:
        return None
    # Divide into contract sections
    if (pre_contracts := divide_sections(contents)) is None:
        return None
    # Process each contract
    contracts = {}
    for contract_name, pre_contract in pre_contracts.items():
        if (contract := compile(pre_contract, constants)) is None:
            return None
        contracts[contract_name] = contract
    return contracts

def main(argv):
    parser = argparse.ArgumentParser()
    parser.add_argument("filename", help="VM PISM file: proofs/vm.pism")
    parser.add_argument("--output", type=argparse.FileType('wb', 0),
                        default=sys.stdout.buffer, help="Output file")
    group = parser.add_mutually_exclusive_group()
    group.add_argument('--display', action='store_true',
                       help="show the compiled code in human readable format")
    group.add_argument('--rust', action='store_true',
                       help="output compiled code to rust for testing")
    group.add_argument('--supervisor', action='store_true',
                       help="output compiled code to zkvm supervisor")
    args = parser.parse_args()

    src_filename = args.filename
    contents = open(src_filename).read()
    if (contracts := process(contents)) is None:
        return -2

    def default_display():
        for contract_name, contract in contracts.items():
            print("Contract:", contract_name)
            print(contract)

    if args.display:
        default_display()
    elif args.rust:
        import compile_export_rust
        for contract_name, contract in contracts.items():
            compile_export_rust.display(contract)
    elif args.supervisor:
        import compile_export_supervisor
        for contract_name, contract in contracts.items():
            compile_export_supervisor.export(args.output, contract_name,
                                             contract)
    else:
        default_display()

    return 0

if __name__ == "__main__":
    sys.exit(main(sys.argv))

