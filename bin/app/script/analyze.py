#!/usr/bin/python
from traxator import *
import sys

if len(sys.argv) != 2:
    print("wrong args", file=sys.stderr)
    sys.exit(-1)
fname = sys.argv[1]

epoch = 1

sections = read_trax(fname)

def check(id, i):
    for j, sect in enumerate(sections):
        if j >= i:
            break
        match sect:
            case DelBuf(epoch, buf, tag, buftype, stat):
                assert buf != id

def checktex(id, i):
    for j, sect in enumerate(sections):
        if j >= i:
            break
        match sect:
            case DelTex(epoch, buf, tag, stat):
                assert buf != id


for i, sect in enumerate(sections):
    match sect:
        case PutDrawCall(_, _, dcs, stats):
            for dc in dcs:
                #print(f"DrawCall {dc.dc_id}")
                for (i, instr) in enumerate(dc.instrs):
                    match instr:
                        case Draw(vert_id, ve, _, _, idx_id, ie, _, _, tex, _):
                            assert ve == ie
                            if ve != epoch:
                                continue
                            if tex:
                                (tex_id, _, _) = tex
                                checktex(tex_id, i)
                            check(vert_id, i)
                            check(idx_id, i)

