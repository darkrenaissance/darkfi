#!/usr/bin/python
import zmq
from pydrk import serial

context = zmq.Context()
socket = context.socket(zmq.REQ)
#self.socket.setsockopt(zmq.IPV6, True)
socket.connect(f"tcp://127.0.0.1:9484")

req_cmd = bytearray()
serial.write_u8(req_cmd, 0)
payload = bytearray()
socket.send_multipart([req_cmd, payload])

errc, reply = socket.recv_multipart()
errc = int.from_bytes(errc, "little")
cursor = serial.Cursor(reply)
response = serial.decode_str(cursor)
print(errc, response)

