from collections import namedtuple

import zmq

from . import serial
from .api import Api

def make_sub_socket(addr="localhost", port=9485):
    context = zmq.Context()
    socket = context.socket(zmq.SUB)
    socket.setsockopt(zmq.SUBSCRIBE, b'')
    socket.connect(f"tcp://{addr}:{port}")
    return socket

KeyMods = namedtuple("KeyMods", ["shift", "ctrl", "alt", "logo"])

class EventLoop:
    """Subscribes to scene-graph signals over the netdebug PUB socket and
    dispatches them to overridable handlers. Register extra slots in a
    subclass constructor via self.register_slot()."""

    def __init__(self, api, addr="localhost"):
        self.api = api
        self.subsock = make_sub_socket(addr)
        self.register_slot("/window/input/keyboard", "key_down", b"kd")

    def register_slot(self, node_path, sig, tag):
        self.api.register_slot(node_path, sig, "", tag)

    def run(self):
        while True:
            signal_data, user_data = self.subsock.recv_multipart()
            cur = serial.Cursor(signal_data)
            match user_data:
                case b"kd":
                    shift = bool(serial.read_u8(cur))
                    ctrl = bool(serial.read_u8(cur))
                    alt = bool(serial.read_u8(cur))
                    logo = bool(serial.read_u8(cur))
                    repeat = bool(serial.read_u8(cur))
                    keycode = serial.decode_str(cur)

                    keymods = KeyMods(shift, ctrl, alt, logo)
                    # Sometimes these get stuck when exiting the window.
                    # We don't need these anyway
                    if keycode in ("LeftShift", "LeftSuper"):
                        continue
                    self.key_down(keycode, keymods, repeat)

    def key_down(self, keycode, keymods, repeat):
        pass
