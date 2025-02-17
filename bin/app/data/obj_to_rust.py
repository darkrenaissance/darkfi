#!/usr/bin/env python

import sys

class Obj:

    def __init__(self, name):
        self.name = name
        self.v = []
        self.f = []

    def push_vert(self, args):
        assert len(args) == 3
        (x, y, z) = [float(v) for v in args]
        assert y == 0
        vert = (x, z)
        self.v.append(vert)

    def push_face(self, args):
        idxs = [arg.split("/")[0] for arg in args]
        assert len(idxs) == 3
        idxs = [int(idx) - 1 for idx in idxs]
        for idx in idxs:
            assert idx < len(self.v)
        self.f.extend(idxs)

def parse_obj(fname):
    objs = []
    obj = None

    for line in open(fname):
        line = line.rstrip("\n")
        if line[0] == '#':
            continue

        line = line.split(" ")
        cmd = line[0]
        args = line[1:]

        match cmd:
            case 'o':
                if obj is not None:
                    objs.append(obj)
                name = args[0]
                #print(f"New object {name}")
                obj = Obj(name)
                continue
            case 'v':
                obj.push_vert(args)
            case 'f':
                obj.push_face(args)

            case _:
                #print(f"Skipping {cmd}: {args}")
                pass

    if obj is not None:
        objs.append(obj)
    return objs

def output(obj):
    name = obj.name
    print("use crate::{mesh::Color, ui::{VectorShape, ShapeVertex}};")
    print(f"pub fn create_{name}(color: Color) -> VectorShape {{")
    print("    VectorShape {")
    print("        verts: vec![")
    for (x, y) in obj.v:
        print(f"            ShapeVertex::from_xy({x}, {y}, color),")
    print("        ],")
    indices = ", ".join([str(f) for f in obj.f])
    print(f"        indices: vec![{indices}]")
    print("    }")
    print("}")

def main(argv):
    if len(argv) != 2:
        print("obj_to_rust OBJFILE", file=sys.stderr)
        return -1

    objs = parse_obj(argv[1])

    for obj in objs:
        output(obj)

    return 0

if __name__ == "__main__":
    sys.exit(main(sys.argv))

