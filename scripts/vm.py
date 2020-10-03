import sys
from enum import Enum

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
        return self.text.split(" ")[1:]

def clean(contents):
    # Split input into lines
    contents = contents.split("\n")
    contents = [Line(line, i) for i, line in enumerate(contents)]
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

alloc_commands = {
    "param": 1,
    "private": 1,
    "public": 1,
}

op_commands = {
    "set": 2,
    "mul": 2,
}

constraint_commands = {
    "lc0_add": 1,
    "lc1_add": 1,
    "lc2_add": 1,
    "lc0_add_one": 0,
    "lc1_add_one": 0,
    "lc2_add_one": 0,
    "enforce": 0,
}

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

        alloc_table[symbol] = Variable(symbol, i, type, is_param)

    return alloc_table

def symbols_list_to_indexes(line, alloc):
    indexes = []
    for symbol in line.args():
        if symbol not in alloc:
            eprint("error: missing unallocated symbol")
            eprint(line)
            return None

        # Lookup variable index
        index = alloc[symbol].index
        indexes.append(index)
    return indexes

class Operation:

    def __init__(self, line, indexes):
        self.command = line.command()
        self.args = indexes
        self.line = line

def generate_ops_table(contract, alloc):
    relevant_lines = extract_relevant_lines(contract, op_commands)
    ops = []
    for line in relevant_lines:
        indexes = symbols_list_to_indexes(line, alloc)
        ops.append(Operation(line, indexes))
    return ops

class Constraint:

    def __init__(self, line, indexes):
        self.command = line.command()
        self.args = indexes
        self.line = line

    def args_comment(self):
        return ", ".join("%s" % symbol for symbol in self.line.args())

def generate_constraints_table(contract, alloc):
    relevant_lines = extract_relevant_lines(contract, constraint_commands)
    constraints = []
    for line in relevant_lines:
        indexes = symbols_list_to_indexes(line, alloc)
        constraints.append(Constraint(line, indexes))
    return constraints

def compile(contract, constants):
    # Allocation table
    # symbol: Private/Public, is_param, index
    alloc = generate_alloc_table(contract)
    # Operations lines list
    if (ops := generate_ops_table(contract, alloc)) is None:
        return False
    # Constraint commands
    if (constraints := generate_constraints_table(contract, alloc)) is None:
        return False
    display(alloc, ops, constraints)
    return True

def display(alloc, ops, constraints):
    print("Alloc table:")
    for symbol, variable in alloc.items():
        print("  //", symbol)
        print(" ", variable.type, variable.index)
    print()

    print("Operations:")
    for op in ops:
        print("  //", op.line)
        print(" ", op.command, op.args)
    print()

    print("Constraints:")
    for constraint in constraints:
        if constraint.args:
            print("  //", constraint.args_comment())
        print(" ", constraint.command, constraint.args)
    print()

def process(contents):
    # Remove left whitespace
    contents = clean(contents)
    # Parse all constants
    constants = [line for line in contents if line.command() == "constant"]
    # Divide into contract sections
    if (contracts := divide_sections(contents)) is None:
        return False
    # Process each contract
    for contract_name, contract in contracts.items():
        if not compile(contract, constants):
            return False
    return True

def main(argv):
    if len(argv) != 2:
        eprint("pism FILENAME")
        return -1

    src_filename = argv[1]
    contents = open(src_filename).read()
    if not process(contents):
        return -2

    return 0

if __name__ == "__main__":
    sys.exit(main(sys.argv))

