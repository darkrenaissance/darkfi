from collections import namedtuple
from pydrk import Api, HostApi, PropertyType
import zmq

api = Api()
host = HostApi(api)
print("Node status:", api.hello())

def make_sub_socket():
    context = zmq.Context()
    socket = context.socket(zmq.SUB)
    socket.setsockopt(zmq.SUBSCRIBE, b'')
    socket.connect("tcp://localhost:9485")
    return socket

def remove_all_slots(node_path, sig):
    node_id = api.lookup_node_id(node_path)
    for slot_id, slot in api.get_slots(node_id, sig):
        print(f"{node_path}:{sig}(): Unregistering slot '{slot}':{slot_id}")
        api.unregister_slot(node_id, sig, slot_id)

def register_slot(node_path, sig, tag):
    remove_all_slots(node_path, sig)
    node_id = api.lookup_node_id(node_path)
    api.register_slot(node_id, sig, "", tag)

def get_property(node_path, prop):
    node_id = api.lookup_node_id(node_path)
    return api.get_property(node_id, prop)

def set_property(node_id, prop, val):
    node_id = _lookup_node(node_id)
    match val:
        case float():
            api.set_property_f32(node_id, prop, val)
        case int():
            api.set_property_u32(node_id, prop, val)
def set_property_f32(node_id, prop, val):
    node_id = _lookup_node(node_id)
    api.set_property_f32(node_id, prop, float(val))
def set_property_u32(node_id, prop, val):
    node_id = _lookup_node(node_id)
    api.set_property_u32(node_id, prop, int(val))

def add_property_bool(node_id, prop, val=None):
    api.add_property(node_id, prop, PropertyType.BOOL)
    if val is not None:
        api.set_property_bool(node_id, prop, val)
def add_property_f32(node_id, prop, val=None):
    api.add_property(node_id, prop, PropertyType.FLOAT32)
    if val is not None:
        api.set_property_f32(node_id, prop, val)
def add_property_u32(node_id, prop, val=None):
    api.add_property(node_id, prop, PropertyType.UINT32)
    if val is not None:
        api.set_property_u32(node_id, prop, val)

def _lookup_node(node_id):
    if isinstance(node_id, str):
        node_id = api.lookup_node_id(node_id)
    return node_id

def link_node(child_id, parent_id):
    child_id = _lookup_node(child_id)
    parent_id = _lookup_node(parent_id)
    api.link_node(child_id, parent_id)
def unlink_node(child_id, parent_id):
    child_id = _lookup_node(child_id)
    parent_id = _lookup_node(parent_id)
    api.unlink_node(child_id, parent_id)

def remove_node_recursive(node_id):
    for (_, child_id, _) in api.get_children(node_id):
        # Unlink the child
        api.unlink_node(child_id, node_id)
        # Remove the node
        remove_node_recursive(child_id)
    # Garbage collection
    if not api.get_parents(node_id):
        api.remove_node(node_id)

def garbage_collect():
    dangling = api.scan_dangling()
    for node_id in dangling:
        remove_node_recursive(node_id)
    print(f"Garbage collect: removed {len(dangling)} nodes")

KeyMods = namedtuple("KeyMods", ["shift", "ctrl", "alt", "logo"])

class EventLoop:

    def __init__(self):
        self.subsock = make_sub_socket()
        register_slot("/window",                "resize",        b"rs")
        register_slot("/window/input/mouse",    "button_down",   b"ck")
        register_slot("/window/input/mouse",    "wheel",         b"wh")
        register_slot("/window/input/mouse",    "move",          b"mm")
        register_slot("/window/input/keyboard", "key_down",      b"kd")

    def run(self):
        while True:
            data = self.subsock.recv()
            match data:
                case b"rs":
                    w = get_property("/window", "width")
                    h = get_property("/window", "height")
                    self.resize_event(w, h)
                case b"ck":
                    x = get_property("/window/input/mouse", "click_x")
                    y = get_property("/window/input/mouse", "click_y")
                    self.mouse_click(x, y)
                case b"wh":
                    y = get_property("/window/input/mouse", "wheel_y")
                    self.mouse_wheel(y)
                case b"mm":
                    pass
                case b"kd":
                    shift =   get_property("/window/input/keyboard", "shift")
                    ctrl =    get_property("/window/input/keyboard", "ctrl")
                    alt =     get_property("/window/input/keyboard", "alt")
                    logo =    get_property("/window/input/keyboard", "logo")
                    keycode = get_property("/window/input/keyboard", "keycode")
                    repeat =  get_property("/window/input/keyboard", "repeat")

                    keymods = KeyMods(shift, ctrl, alt, logo)
                    # Sometimes these get stuck when exiting the window.
                    # We don't need these anyway
                    if keycode in ("LeftShift", "LeftSuper"):
                        continue
                    self.key_down(keycode, keymods, repeat)

    def resize_event(self, w, h):
        pass

    def mouse_click(self, x, y):
        pass

    def mouse_wheel(self, y):
        pass

    def key_down(self, keycode, keymods, repeat):
        pass

