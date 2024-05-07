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
        self.cursor_layer.resize(w, h)

        self.rounded_box_layer = Layer("rounded_box_layer")
        self.rounded_box_layer.add_obj("box")
        self.rounded_box_layer.resize(w, h)

        self.user_input = ""
        self.last_keypress_time = 0

    def resize_event(self, w, h):
        self.chatbox_layer.resize(w, h)
        self.cursor_layer.resize(w, h)
        self.rounded_box_layer.resize(w, h)
        resize_box()
        resize_rounded_box()
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

    reposition_cursor()

def reposition_cursor():
    layer_h = get_property("/window/chatbox_layer", "rect_h")
    y = layer_h - 20 - 30

    # Move the cursor
    text_id = api.lookup_node_id("/window/chatbox_layer/user_input/txt")
    text_px_w = 0
    user_input = ""
    if text_id is not None:
        text_px_w = get_property(text_id, "width")
        user_input = get_property(text_id, "text")
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
    padding = 10

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

def draw_rounded_box():
    mesh_id = api.add_node("box", SceneNodeType.RENDER_MESH)
    api.add_property(mesh_id, "verts", PropertyType.BUFFER)
    api.add_property(mesh_id, "faces", PropertyType.BUFFER)
    link_node(mesh_id, "/window/rounded_box_layer/box")

    mesh_id = api.add_node("inner_box", SceneNodeType.RENDER_MESH)
    api.add_property(mesh_id, "verts", PropertyType.BUFFER)
    api.add_property(mesh_id, "faces", PropertyType.BUFFER)
    link_node(mesh_id, "/window/rounded_box_layer/box")

    resize_rounded_box()

def resize_rounded_box():
    mesh_id = api.lookup_node_id("/window/rounded_box_layer/box/box")

    layer_w = get_property("/window/rounded_box_layer", "rect_w")
    layer_h = get_property("/window/rounded_box_layer", "rect_h")

    # Inner padding, so box inside will be (box_h - 2*padding) px high
    padding = 5

    bevel = 20

    # Lets add a poly - must be counterclockwise
    x, y = layer_w/4, layer_h/4
    w, h = layer_w/2, layer_h/2
    y0 = y + bevel
    h0 = h - 2*bevel
    verts = (
        vertex(x,     y0,     1, 1, 1, 1, 0, 0) +
        vertex(x + w, y0,     1, 1, 1, 1, 1, 0) +
        vertex(x,     y0 + h0, 1, 1, 1, 1, 0, 1) +
        vertex(x + w, y0 + h0, 1, 1, 1, 1, 1, 1)
    )
    faces = face(0, 2, 1) + face(1, 2, 3)
    x0 = x + bevel
    w0 = w - 2*bevel
    h0 = bevel
    verts += (
        vertex(x0,      y,     1, 1, 1, 1, 0, 0) +
        vertex(x0 + w0, y,     1, 1, 1, 1, 1, 0) +
        vertex(x0,      y + h0, 1, 1, 1, 1, 0, 1) +
        vertex(x0 + w0, y + h0, 1, 1, 1, 1, 1, 1)
    )
    o = 4
    faces += face(o + 0, o + 2, o + 1) + face(o + 1, o + 2, o + 3)
    y0 = y + h - bevel
    x0 = x + bevel
    w0 = w - 2*bevel
    h0 = bevel
    verts += (
        vertex(x0,      y0,     1, 1, 1, 1, 0, 0) +
        vertex(x0 + w0, y0,     1, 1, 1, 1, 1, 0) +
        vertex(x0,      y0 + h0, 1, 1, 1, 1, 0, 1) +
        vertex(x0 + w0, y0 + h0, 1, 1, 1, 1, 1, 1)
    )
    o = 8
    faces += face(o + 0, o + 2, o + 1) + face(o + 1, o + 2, o + 3)
    api.set_property_buffer(mesh_id, "verts", verts)
    api.set_property_buffer(mesh_id, "faces", faces)

    # Second mesh
    mesh_id = api.lookup_node_id("/window/rounded_box_layer/box/inner_box")

    x, y = x + padding, y + padding
    w -= 2*padding
    h -= 2*padding
    vert1 = vertex(x,     y,     0, 0, 0, 1, 0, 0)
    vert2 = vertex(x + w, y,     0, 0, 0, 1, 1, 0)
    vert3 = vertex(x,     y + h, 0, 0, 0, 1, 0, 1)
    vert4 = vertex(x + w, y + h, 0.3, 0.3, 0.3, 1, 1, 1)
    #api.set_property_buffer(mesh_id, "verts", vert1 + vert2 + vert3 + vert4)
    #api.set_property_buffer(mesh_id, "faces", face(0, 2, 1) + face(1, 2, 3))

def main():
    if True:
        node_id = api.add_node("foo", SceneNodeType.WINDOW)
        prop = Property(
            "myprop", PropertyType.FLOAT32, PropertySubType.NULL,
            None,
            "myprop", "",
            False, 2, None, None, []
        )
        api.add_property(1, prop)
        api.link_node(node_id, 0)
        api.set_property_f32(1, "myprop", 0, 4.0)
        api.set_property_f32(1, "myprop", 1, 110.0)
    print("val =", api.get_property_value(1, "myprop"))
    for prop in api.get_properties(1):
        print("Property:")
        print(f"  name = {prop.name}")
        print(f"  type = {prop.type}")
        print(f"  subtype = {prop.subtype}")
        print(f"  defaults = {prop.defaults}")
        print(f"  ui_name = {prop.ui_name}")
        print(f"  desc = {prop.desc}")
        print(f"  is_null_allowed = {prop.is_null_allowed}")
        print(f"  array_len = {prop.array_len}")
        print(f"  min_val = {prop.min_val}")
        print(f"  max_val = {prop.max_val}")
        print(f"  enum_items = {prop.enum_items}")
        print()
    print_tree()
    #garbage_collect()

    #app = App()
    #draw_box()
    #draw_rounded_box()
    #draw_cursor()
    #reposition_cursor()
    ##print_tree()
    #app.run()

