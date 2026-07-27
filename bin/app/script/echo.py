#!/usr/bin/env python

import zmq
from pydrk import serial

context = zmq.Context()
socket = context.socket(zmq.REQ)
#socket.setsockopt(zmq.IPV6, True)
# Need to specify network interface for IPv6 link-local addrs
#socket.connect(f"tcp://[XXX%eth1]:9484")
socket.connect(f"tcp://[::]:9484")

req_cmd = bytearray()
serial.write_u8(req_cmd, 0)
payload = bytearray()
socket.send_multipart([req_cmd, payload])

errc, reply = socket.recv_multipart()
errc = int.from_bytes(errc, "little")
cursor = serial.Cursor(reply)
response = serial.decode_str(cursor)
print(errc, response)

