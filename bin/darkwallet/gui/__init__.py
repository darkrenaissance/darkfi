from pydrk import SceneNodeType, PropertyType, vertex, face
from .print_tree import print_tree
from .api import *
from .gfx import Layer, add_object
from . import settings
import time

latin = "ABCDEFGHIJKLMNOPQRSTUVWXYZ"
numerals = "0123456789"
punct = " .,<>/\\'[]{}:`~!@#$%^&*()_+?"
keycmds = [
    "Backspace"
]

class App(EventLoop):

    def __init__(self):
        super().__init__()

        w = get_property("/window", "width")
        h = get_property("/window", "height")

        self.chatbox_layer = Layer("chatbox_layer")
        self.chatbox_layer.resize(w, h)

        self.chatbox_layer.add_obj("outline")
        self.chatbox_layer.add_obj("user_input")

        self.cursor_layer = Layer("cursor_layer")
        self.cursor_layer.add_obj("cursor")

        self.user_input = ""
        self.last_keypress_time = 0

    def resize_event(self, w, h):
        self.chatbox_layer.resize(w, h)
        self.cursor_layer.resize(w, h)
        resize_box()
        draw_txt(self.user_input)

    def mouse_click(self, x, y):
        print(f"mouse click ({x}, {y})")

    def mouse_wheel(self, y):
        settings.ui_scale *= 1 + y/100
        print(settings.ui_scale)

    def key_down(self, keycode, keymods, repeat):
        #print(f"key_down: '{keycode}'")
        if keymods.ctrl and keycode == "=":
            settings.ui_scale *= 1.01
            print(settings.ui_scale)
        elif keymods.ctrl and keycode == "-":
            settings.ui_scale *= 0.99
            print(settings.ui_scale)
        elif keymods.ctrl and keycode == "H":
            print("hello")
        elif keymods.ctrl and keycode == "P":
            print_tree()

        elif keycode in latin:
            key = keycode.upper() if keymods.shift else keycode.lower()
            self.type_key(key, keymods, repeat)
        elif keycode in numerals or keycode in punct or keycode in keycmds:
            self.type_key(keycode, keymods, repeat)
        #else:
        #    print(keycode)

    def type_key(self, key, _keymods, _repeat):
        now = time.time()
        if now - self.last_keypress_time < 0.2:
            return
        self.last_keypress_time = now
        if key == "Backspace":
            self.user_input = self.user_input[:-1]
        else:
            self.user_input += key
        draw_txt(self.user_input)

def draw_txt(user_input):
    obj_id = add_object("/window/chatbox_layer", "user_input2")
    # create a new one and link it
    text_id = host.create_text("/font/inter-regular", "txt2", user_input, 30)
    link_node(text_id, obj_id)

    layer_w = get_property("/window/chatbox_layer", "rect_w")
    layer_h = get_property("/window/chatbox_layer", "rect_h")

    x = 20
    y = layer_h - 20 - 30

    set_property_f32("/window/chatbox_layer/user_input2", "x", x)
    set_property_f32("/window/chatbox_layer/user_input2", "y", y)
    set_property_f32("/window/chatbox_layer/user_input2/txt2", "r", 1)
    set_property_f32("/window/chatbox_layer/user_input2/txt2", "g", 1)
    set_property_f32("/window/chatbox_layer/user_input2/txt2", "b", 1)
    set_property_f32("/window/chatbox_layer/user_input2/txt2", "a", 1)

    # Switch visibility
    set_property_bool("/window/chatbox_layer/user_input",  "is_visible", False)
    set_property_bool("/window/chatbox_layer/user_input2", "is_visible", True)

    # Remove the old object
    old_id = lookup_node("/window/chatbox_layer/user_input")
    unlink_from_parents(old_id)
    remove_node_recursive(old_id)

    rename_node("/window/chatbox_layer/user_input2/txt2", "txt")
    rename_node("/window/chatbox_layer/user_input2",      "user_input")

    # Move the cursor
    text_px_w = get_property("/window/chatbox_layer/user_input/txt", "width")
    x = text_px_w + 25
    if user_input and user_input[-1] == " ":
        x += 10
    set_property_f32("/window/cursor_layer/cursor", "x", x)
    set_property_f32("/window/cursor_layer/cursor", "y", y)

def draw_cursor():
    mesh_id = api.add_node("cursor_box", SceneNodeType.RENDER_MESH)
    api.add_property(mesh_id, "verts", PropertyType.BUFFER)
    api.add_property(mesh_id, "faces", PropertyType.BUFFER)
    link_node(mesh_id, "/window/cursor_layer/cursor")

    x, y = 0, 0
    w, h = 20, 40
    vert1 = vertex(x,     y,     1, 1, 1, 1, 0, 0)
    vert2 = vertex(x + w, y,     1, 1, 1, 1, 1, 0)
    vert3 = vertex(x,     y + h, 1, 1, 1, 1, 0, 1)
    vert4 = vertex(x + w, y + h, 1, 1, 1, 1, 1, 1)
    api.set_property_buffer(mesh_id, "verts", vert1 + vert2 + vert3 + vert4)
    api.set_property_buffer(mesh_id, "faces", face(0, 2, 1) + face(1, 2, 3))

def draw_box():
    mesh_id = api.add_node("box", SceneNodeType.RENDER_MESH)
    api.add_property(mesh_id, "verts", PropertyType.BUFFER)
    api.add_property(mesh_id, "faces", PropertyType.BUFFER)
    link_node(mesh_id, "/window/chatbox_layer/outline")

    mesh_id = api.add_node("inner_box", SceneNodeType.RENDER_MESH)
    api.add_property(mesh_id, "verts", PropertyType.BUFFER)
    api.add_property(mesh_id, "faces", PropertyType.BUFFER)
    link_node(mesh_id, "/window/chatbox_layer/outline")

    resize_box()

def resize_box():
    mesh_id = api.lookup_node_id("/window/chatbox_layer/outline/box")

    layer_w = get_property("/window/chatbox_layer", "rect_w")
    layer_h = get_property("/window/chatbox_layer", "rect_h")

    box_h = 60
    # Inner padding, so box inside will be (box_h - 2*padding) px high
    padding = 5

    # Lets add a poly - must be counterclockwise
    x, y = 0, layer_h - box_h
    w, h = layer_w, box_h
    vert1 = vertex(x,     y,     1, 1, 1, 1, 0, 0)
    vert2 = vertex(x + w, y,     1, 1, 1, 1, 1, 0)
    vert3 = vertex(x,     y + h, 1, 1, 1, 1, 0, 1)
    vert4 = vertex(x + w, y + h, 1, 1, 1, 1, 1, 1)
    api.set_property_buffer(mesh_id, "verts", vert1 + vert2 + vert3 + vert4)
    api.set_property_buffer(mesh_id, "faces", face(0, 2, 1) + face(1, 2, 3))

    # Second mesh
    mesh_id = api.lookup_node_id("/window/chatbox_layer/outline/inner_box")

    x, y = x + padding, y + padding
    w -= 2*padding
    h -= 2*padding
    vert1 = vertex(x,     y,     0, 0, 0, 1, 0, 0)
    vert2 = vertex(x + w, y,     0, 0, 0, 1, 1, 0)
    vert3 = vertex(x,     y + h, 0, 0, 0, 1, 0, 1)
    vert4 = vertex(x + w, y + h, 0.3, 0.3, 0.3, 1, 1, 1)
    api.set_property_buffer(mesh_id, "verts", vert1 + vert2 + vert3 + vert4)
    api.set_property_buffer(mesh_id, "faces", face(0, 2, 1) + face(1, 2, 3))

def main():
    garbage_collect()

    app = App()
    draw_box()
    draw_cursor()
    print_tree()
    app.run()

