#!/usr/bin/python
from traxator import *
import math

dc_id = 2767617242841734550
vert_id = 39568
idx_id = 39569

sections = read_trax()
for sect in sections:
    match sect:
        case PutDrawCall(_, _, dcs, stats):
            for dc in dcs:
                if dc.dc_id == dc_id:
                    print(f"DrawCall {dc_id}")
                    for (i, instr) in enumerate(dc.instrs):
                        print(f"  {i}. {instr}")
                    print()
        case PutVerts(_, verts, buf, _, _, _):
            if buf == vert_id:
                print(f"Vert {vert_id}")
                print("  n verts:", len(verts))
                print(verts)
                print()
                #for v in verts:
                #    assert not math.isnan(v.x)
                #    assert not math.isnan(v.y)
                #    assert not math.isnan(v.r)
                #    assert not math.isnan(v.g)
                #    assert not math.isnan(v.b)
                #    assert not math.isnan(v.a)
                #    assert not math.isnan(v.u)
                #    assert not math.isnan(v.v)

                #    assert not math.isinf(v.x)
                #    assert not math.isinf(v.y)
                #    assert not math.isinf(v.r)
                #    assert not math.isinf(v.g)
                #    assert not math.isinf(v.b)
                #    assert not math.isinf(v.a)
                #    assert not math.isinf(v.u)
                #    assert not math.isinf(v.v)
        case PutIdxs(_, idxs, buf, _, _, _):
            if buf == idx_id:
                print(f"Idx {idx_id}")
                print(idxs)
                print()

