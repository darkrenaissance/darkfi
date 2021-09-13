import os.path
import sys
from jinja2 import Environment, FileSystemLoader, Template

def main(argv):
    if len(argv) != 2:
        print("error: missing arg", file=sys.stderr)
        return -1

    path = argv[1]
    dirname, filename = os.path.dirname(path), os.path.basename(path)
    env = Environment(loader = FileSystemLoader([dirname]))
    template = env.get_template(filename)
    print(template.render())

    return 0

if __name__ == "__main__":
    sys.exit(main(sys.argv))

