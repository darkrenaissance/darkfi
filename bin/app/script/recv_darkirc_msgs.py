#!/usr/bin/python3
import zmq
from pydrk import api, serial
from datetime import datetime

api_client = api.Api(addr="127.0.0.1", port=9484)

context = zmq.Context()
pub_socket = context.socket(zmq.SUB)
pub_socket.connect("tcp://127.0.0.1:9485")
pub_socket.setsockopt(zmq.SUBSCRIBE, b"")

node_path = "/plugin/darkirc"
sig_name = "recv"
slot_name = "python_listener"
user_data = b"darkirc_recv"

slot_id = api_client.register_slot(node_path, sig_name, slot_name, user_data)
print(f"Registered slot ID: {slot_id}")
print("Listening...")
print("=" * 80)

while True:
    parts = pub_socket.recv_multipart()
    assert len(parts) == 2
    signal_data, recv_user_data = parts
    assert recv_user_data == user_data

    cur = serial.Cursor(signal_data)
    channel = serial.decode_str(cur)
    timestamp = serial.read_u64(cur)
    msg_id_bytes = cur.read(32)
    msg_id = msg_id_bytes.hex()
    nick = serial.decode_str(cur)
    msg = serial.decode_str(cur)

    dt = datetime.fromtimestamp(timestamp / 1000.0)
    print(f"[{dt.strftime('%Y-%m-%d %H:%M:%S')}] #{channel} <{nick}> {msg}")
    print(f"  Message ID: {msg_id}")
    print("-" * 80)
