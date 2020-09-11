import sys

class LineDesc:

    def __init__(self, level, text):
        self.level = level
        self.text = text
        assert self.text[0] != ' '

def parse(text):
    lines = text.split("\n")

    linedescs = []
    for line in lines[:50]:
        lead_spaces = len(line) - len(line.lstrip(" "))
        level = lead_spaces / 4
        if line.strip(" ") == "":
            continue
        line = line.lstrip(" ")
        print(level, line)
        ldesc = LineDesc(level, line)
        linedescs.append(ldesc)

def main(argv):
    if len(argv) == 1:
        print("error: missing proof file", file=sys.stderr)
        return -1
    filename = sys.argv[1]
    text = open(filename, "r").read()
    parse(text)

if __name__ == "__main__":
    main(sys.argv)

