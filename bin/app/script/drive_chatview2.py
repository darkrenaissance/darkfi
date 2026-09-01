#!/usr/bin/env python3
# Drives the single-screen chatview2 cutover over netdebug:
# set_channel, insert lines, switch channels, verify per-channel state.
import sys
sys.path.insert(0, "pydrk")
from pydrk.api import Api
from pydrk import serial as s

CHATTY = "/window/content/chat/main_chat_layer/content/chatty"
PRIV = CHATTY + "/privmsg"


def enc_insert(ts, mid, nick, text):
    b = bytearray()
    s.write_u64(b, ts)
    b += mid
    s.encode_str(b, nick)
    s.encode_str(b, text)
    return bytes(b)


def dec_line_ids(data):
    cur = s.Cursor(bytearray(data))
    out = []
    while cur.i < len(cur.by):
        ts = s.read_u64(cur)
        mid = bytes(cur.read(32))
        out.append((ts, mid[:4].hex()))
    return out


api = Api()
api.hello()
print("== scene has chatview2:", api.get_children(CHATTY)[0][0] if api.get_children(CHATTY) else None)

# Bind #dev and insert history
api.call_method(CHATTY, "set_channel", s.encode_str(bytearray(), "#dev"))
for i in range(5):
    api.call_method(
        PRIV, "insert_line",
        enc_insert(1_756_000_000_000 + i * 60_000, bytes([i]) + b"\x00" * 31, "alice", f"dev message {i}"),
    )

res = api.call_method(CHATTY, "get_line_ids", b"")
lines = dec_line_ids(res) if res else []
print(f"== #dev lines: {len(lines)}")
for ts, mid in lines:
    print("  ", ts, mid)

# Switch to #random, insert 2 lines
api.call_method(CHATTY, "set_channel", s.encode_str(bytearray(), "#random"))
for i in range(2):
    api.call_method(
        PRIV, "insert_line",
        enc_insert(1_756_000_100_000 + i * 60_000, bytes([10 + i]) + b"\x00" * 31, "bob", f"random message {i}"),
    )
res = api.call_method(CHATTY, "get_line_ids", b"")
print(f"== #random lines: {len(dec_line_ids(res) if res else [])}")

# Back to #dev: its lines must be back (buffer reload), newest first
api.call_method(CHATTY, "set_channel", s.encode_str(bytearray(), "#dev"))
res = api.call_method(CHATTY, "get_line_ids", b"")
lines = dec_line_ids(res) if res else []
print(f"== #dev lines after round-trip: {len(lines)} (expect 5)")
assert len(lines) == 5, "channel state not restored"

# is_at_bottom property readable
v = api.get_property_value(CHATTY, "is_at_bottom")
print("== is_at_bottom:", v)

# delete_line removes one and it survives the next reload
mid4 = bytes([4]) + b"\x00" * 31
b = bytearray()
b += mid4
api.call_method(CHATTY, "delete_line", bytes(b))
api.call_method(CHATTY, "set_channel", s.encode_str(bytearray(), "#dev"))
res = api.call_method(CHATTY, "get_line_ids", b"")
lines = dec_line_ids(res) if res else []
print(f"== #dev lines after delete+reload: {len(lines)} (expect 4)")
assert len(lines) == 4

print("OK")
