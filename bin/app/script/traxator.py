#!/usr/bin/python
from pydrk import serial
from collections import namedtuple
from dataclasses import dataclass
from typing import Union

@dataclass
class SetScale:
    scale: float

@dataclass
class Move:
    x: float
    y: float

@dataclass
class SetPos:
    x: float
    y: float

@dataclass
class ApplyView:
    x: float
    y: float
    w: float
    h: float

@dataclass
class Draw:
    vert_id: int
    vert_epoch: int
    vert_tag: str
    vert_buftype: int
    index_id: int
    index_epoch: int
    index_tag: str
    index_buftype: int
    tex: (int, int, str)
    num_elements: int

Instr = Union[SetScale, Move, SetPos, ApplyView, Draw]

def hex(dat):
    return " ".join(f"{b:02x}" for b in dat)

def read_tag(cur):
    return serial.decode_str(cur)

DrawCall = namedtuple("DrawCall", [
    "dc_id",
    "instrs",
    "dcs",
    "z_index",
    "debug_str"
])

def read_dc(cur):
    dc_id = serial.read_u64(cur)
    instrs = serial.decode_arr(cur, read_instr)
    dcs = serial.decode_arr(cur, serial.read_u64)
    z_index = serial.read_u32(cur)
    debug_str = serial.decode_str(cur)
    return DrawCall(
        dc_id,
        instrs,
        dcs,
        z_index,
        debug_str
    )

def read_instr(cur):
    enum = serial.read_u8(cur)
    match enum:
        case 0:
            scale = serial.read_f32(cur)
            return SetScale(scale)
        case 1:
            x = serial.read_f32(cur)
            y = serial.read_f32(cur)
            return Move(x, y)
        case 2:
            x = serial.read_f32(cur)
            y = serial.read_f32(cur)
            #print(f"  set_pos x={x}, y={y}")
            return SetPos(x, y)
        case 3:
            x = serial.read_f32(cur)
            y = serial.read_f32(cur)
            w = serial.read_f32(cur)
            h = serial.read_f32(cur)
            #print(f"  apply_view x={x}, y={y}, w={w}, h={h}")
            return ApplyView(x, y, w, h)
        case 4:
            vert_id = serial.read_u32(cur)
            vert_epoch = serial.read_u32(cur)
            vert_tag = serial.decode_opt(cur, read_tag)
            vert_buftype = serial.read_u8(cur)
            index_id = serial.read_u32(cur)
            index_epoch = serial.read_u32(cur)
            index_tag = serial.decode_opt(cur, read_tag)
            index_buftype = serial.read_u8(cur)
            def read_tex(cur):
                id = serial.read_u32(cur)
                epoch = serial.read_u32(cur)
                tag = serial.decode_opt(cur, read_tag)
                return (id, epoch, tag)
            tex = serial.decode_opt(cur, read_tex)
            num_elements = serial.read_i32(cur)
            return Draw(
                vert_id,
                vert_epoch,
                vert_tag,
                vert_buftype,
                index_id,
                index_epoch,
                index_tag,
                index_buftype,
                tex,
                num_elements
            )
        case _:
            raise NotImplementedError

@dataclass
class Vertex:
    x: float
    y: float
    r: float
    g: float
    b: float
    a: float
    u: float
    v: float

@dataclass
class PutDrawCall:
    epoch: int
    timest: int
    dcs: [DrawCall]
    stats: [int]

@dataclass
class PutTex:
    epoch: int
    tex: int
    tag: str
    stat: int

@dataclass
class PutVerts:
    epoch: int
    verts: [Vertex]
    buf: int
    tag: str
    buftype: int
    stat: int

@dataclass
class PutIdxs:
    epoch: int
    idxs: [int]
    buf: int
    tag: str
    buftype: int
    stat: int

@dataclass
class DelTex:
    epoch: int
    buf: int
    tag: str
    stat: int

@dataclass
class DelBuf:
    epoch: int
    buf: int
    tag: str
    buftype: int
    stat: int

@dataclass
class SetCurr:
    dc: int

@dataclass
class SetInstr:
    idx: int

Section = Union[PutDrawCall, PutTex, PutVerts, PutIdxs, DelTex, DelBuf, SetCurr, SetInstr]

def read_vert(cur):
    return Vertex(
        serial.read_f32(cur),
        serial.read_f32(cur),
        serial.read_f32(cur),
        serial.read_f32(cur),
        serial.read_f32(cur),
        serial.read_f32(cur),
        serial.read_f32(cur),
        serial.read_f32(cur),
    )

def read_section(f):
    fpos = f.tell()
    buf = serial.decode_buf(f)
    if not buf:
        return None
    cur = serial.Cursor(buf)
    c = serial.read_u8(cur)
    #print(f"SECTION: {c} {len(buf)}B [{fpos}]")
    #print(hex(cur.by))
    match c:
        case 0:
            epoch = serial.read_u32(cur)
            timest = serial.read_u64(cur)
            dcs = serial.decode_arr(cur, read_dc)
            stats = []
            for _ in dcs:
                stat = serial.read_u8(cur)
                stats.append(stat)
                #print(f"  stat={stat}")
            #print(f"put_dcs epoch={epoch}, timest={timest}, dcs={dcs}, stats={stats}")
            sect = PutDrawCall(epoch, timest, dcs, stats)
        case 1:
            epoch = serial.read_u32(cur)
            tex = serial.read_u32(cur)
            tag = serial.decode_opt(cur, read_tag)
            stat = serial.read_u8(cur)
            #print(f"put_tex epoch={epoch}, tex={tex}, tag='{tag}', stat={stat}")
            sect = PutTex(epoch, tex, tag, stat)
        case 2:
            epoch = serial.read_u32(cur)
            verts = serial.decode_arr(cur, read_vert)
            buf = serial.read_u32(cur)
            tag = serial.decode_opt(cur, read_tag)
            buftype = serial.read_u8(cur)
            stat = serial.read_u8(cur)
            #print(f"put_verts epoch={epoch}, buf={buf}, tag='{tag}', buftype={buftype}, stat={stat}")
            sect = PutVerts(epoch, verts, buf, tag, buftype, stat)
        case 3:
            epoch = serial.read_u32(cur)
            idxs = serial.decode_arr(cur, serial.read_u16)
            buf = serial.read_u32(cur)
            tag = serial.decode_opt(cur, read_tag)
            buftype = serial.read_u8(cur)
            stat = serial.read_u8(cur)
            #print(f"put_idxs epoch={epoch}, buf={buf}, tag='{tag}', buftype={buftype}, stat={stat}")
            sect = PutIdxs(epoch, idxs, buf, tag, buftype, stat)
        case 4:
            epoch = serial.read_u32(cur)
            buf = serial.read_u32(cur)
            tag = serial.decode_opt(cur, read_tag)
            stat = serial.read_u8(cur)
            #print(f"del_tex epoch={epoch}, buf={buf}, tag='{tag}', stat={stat}")
            sect = DelTex(epoch, buf, tag, stat)
        case 5:
            epoch = serial.read_u32(cur)
            buf = serial.read_u32(cur)
            tag = serial.decode_opt(cur, read_tag)
            buftype = serial.read_u8(cur)
            stat = serial.read_u8(cur)
            #print(f"del_buf epoch={epoch}, buf={buf}, tag='{tag}', buftype={buftype}, stat={stat}")
            sect = DelBuf(epoch, buf, tag, buftype, stat)
        case 6:
            dc = serial.read_u64(cur)
            #print(f"set_curr dc={dc}")
            sect = SetCurr(dc)
        case 7:
            idx = serial.read_u64(cur)
            #print(f"set_instr idx={idx}")
            sect = SetInstr(idx)
        case _:
            raise NotImplementedError

    # Crash out if we didn't fully consume the buffer
    if not cur.is_end():
        print(hex(cur.remain_data()))
    assert cur.is_end()

    return sect

def read_trax():
    f = open("trax.dat", "rb")
    sections = []
    while True:
        if (sect := read_section(f)) is None:
            break
        sections.append(sect)
    return sections

if __name__ == "__main__":
    sections = read_trax()
    for sect in sections:
        print(sect)

