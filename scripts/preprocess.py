import sys
from jinja2 import Template

def main(argv):
    if len(argv) != 2:
        print("error: missing arg", file=sys.stderr)
        return -1

    input = open(argv[1]).read()

    template = Template(input)
    print(template.render())

    return 0

if __name__ == "__main__":
    sys.exit(main(sys.argv))

