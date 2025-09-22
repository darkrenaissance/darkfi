import struct

def write_u8(by, v):
    assert v < 2**256
    by += v.to_bytes(1, 'little')

def write_u16(by, v):
    assert v < 2**(2*256)
    by += v.to_bytes(2, 'little')

def write_u32(by, v):
    assert v < 2**(4*256)
    by += v.to_bytes(4, 'little')

def write_u64(by, v):
    assert v < 2**(8*256)
    by += v.to_bytes(8, 'little')

def write_f32(by, v):
    by += struct.pack("<f", v)

def encode_varint(by, v):
    if v <= 0xfc:
        write_u8(by, v)
    elif v <= 0xffff:
        write_u8(by, 0xfd)
        write_u16(by, v)
    elif v <= 0xffffffff:
        write_u8(by, 0xfe)
        write_u32(by, v)
    else:
        write_u8(by, 0xff)
        write_u64(by, v)

def encode_str(by, s):
    data = s.encode("utf-8")
    encode_buf(by, data)

def encode_buf(by, buf):
    encode_varint(by, len(buf))
    by += buf

def encode_opt(by, val, write_fn):
    if val is None:
        write_u8(by, 0)
    else:
        write_u8(by, 1)
        write_fn(by)

# Cursor for bytearray type
class Cursor:

    def __init__(self, by):
        self.by = by
        self.i = 0

    def read(self, n):
        slice = self.by[self.i:self.i+n]
        self.i += n
        if self.i > len(self.by):
            raise Exception("invalid read")
        return slice

    def remain_data(self):
        return self.by[self.i:]

    def is_end(self):
        return not bool(self.remain_data())

def read_u8(cur):
    b = cur.read(1)
    return int.from_bytes(b, "little")

def read_u16(cur):
    b = cur.read(2)
    return int.from_bytes(b, "little")

def read_u32(cur):
    b = cur.read(4)
    return int.from_bytes(b, "little")

def read_u64(cur):
    b = cur.read(8)
    return int.from_bytes(b, "little")

def read_f32(cur):
    by = cur.read(4)
    return struct.unpack("<f", by)[0]

def read_i32(cur):
    by = cur.read(4)
    return struct.unpack("<i", by)[0]

def decode_varint(cur):
    n = read_u8(cur)
    match n:
        case 0xff:
            x = read_u64(cur)
            assert x >= 0x100000000
            return x
        case 0xfe:
            x = read_u32(cur)
            assert x >= 0x10000
            return x
        case 0xfd:
            x = read_u16(cur)
            assert x >= 0xfd
            return x
    return n

def decode_str(cur):
    return decode_buf(cur).decode("utf-8")

def decode_buf(cur):
    size = decode_varint(cur)
    return cur.read(size)

def decode_opt(cur, read_fn):
    is_some = bool(read_u8(cur))
    if is_some:
        return read_fn(cur)
    else:
        return None

def decode_arr(cur, read_fn):
    arr_len = decode_varint(cur)
    vals = []
    for _ in range(arr_len):
        vals.append(read_fn(cur))
    return vals
