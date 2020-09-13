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
        value_map: NAME ":" type_def

        type_def:   point
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

def read_consts(consts):
    for subsection in consts:
        assert subsection[0].text == "const:"

        for ldesc in subsection[1:]:
            tokens = tokenize_const(ldesc.text)
            print(tokens)

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

    read_consts(consts)

if __name__ == "__main__":
    main(sys.argv)

