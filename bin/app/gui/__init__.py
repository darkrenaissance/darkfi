from pydrk import SceneNodeType, PropertyType, vertex, face, serial
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
    node_id = api.add_node("cursor_box", SceneNodeType.RENDER_MESH)
    api.add_property(node_id, "verts", PropertyType.BUFFER)
    api.add_property(node_id, "faces", PropertyType.BUFFER)
    link_node(node_id, "/window/cursor_layer/cursor")

    x, y = 0, 0
    w, h = 20, 40
    vert1 = vertex(x,     y,     1, 1, 1, 1, 0, 0)
    vert2 = vertex(x + w, y,     1, 1, 1, 1, 1, 0)
    vert3 = vertex(x,     y + h, 1, 1, 1, 1, 0, 1)
    vert4 = vertex(x + w, y + h, 1, 1, 1, 1, 1, 1)
    api.set_property_buffer(node_id, "verts", vert1 + vert2 + vert3 + vert4)
    api.set_property_buffer(node_id, "faces", face(0, 2, 1) + face(1, 2, 3))

def draw_box():
    node_id = api.add_node("box", SceneNodeType.RENDER_MESH)
    api.add_property(node_id, "verts", PropertyType.BUFFER)
    api.add_property(node_id, "faces", PropertyType.BUFFER)
    link_node(node_id, "/window/chatbox_layer/outline")

    node_id = api.add_node("inner_box", SceneNodeType.RENDER_MESH)
    api.add_property(node_id, "verts", PropertyType.BUFFER)
    api.add_property(node_id, "faces", PropertyType.BUFFER)
    link_node(node_id, "/window/chatbox_layer/outline")

    resize_box()

def resize_box():
    node_id = api.lookup_node_id("/window/chatbox_layer/outline/box")

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
    api.set_property_buffer(node_id, "verts", vert1 + vert2 + vert3 + vert4)
    api.set_property_buffer(node_id, "faces", face(0, 2, 1) + face(1, 2, 3))

    # Second mesh
    node_id = api.lookup_node_id("/window/chatbox_layer/outline/inner_box")

    x, y = x + padding, y + padding
    w -= 2*padding
    h -= 2*padding
    vert1 = vertex(x,     y,     0, 0, 0, 1, 0, 0)
    vert2 = vertex(x + w, y,     0, 0, 0, 1, 1, 0)
    vert3 = vertex(x,     y + h, 0, 0, 0, 1, 0, 1)
    vert4 = vertex(x + w, y + h, 0.3, 0.3, 0.3, 1, 1, 1)
    api.set_property_buffer(node_id, "verts", vert1 + vert2 + vert3 + vert4)
    api.set_property_buffer(node_id, "faces", face(0, 2, 1) + face(1, 2, 3))

def draw_rounded_box():
    node_id = api.add_node("box", SceneNodeType.RENDER_MESH)
    api.add_property(node_id, "verts", PropertyType.BUFFER)
    api.add_property(node_id, "faces", PropertyType.BUFFER)
    link_node(node_id, "/window/rounded_box_layer/box")

    node_id = api.add_node("inner_box", SceneNodeType.RENDER_MESH)
    api.add_property(node_id, "verts", PropertyType.BUFFER)
    api.add_property(node_id, "faces", PropertyType.BUFFER)
    link_node(node_id, "/window/rounded_box_layer/box")

    resize_rounded_box()

def resize_rounded_box():
    node_id = api.lookup_node_id("/window/rounded_box_layer/box/box")

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
    api.set_property_buffer(node_id, "verts", verts)
    api.set_property_buffer(node_id, "faces", faces)

    # Second mesh
    node_id = api.lookup_node_id("/window/rounded_box_layer/box/inner_box")

    x, y = x + padding, y + padding
    w -= 2*padding
    h -= 2*padding
    vert1 = vertex(x,     y,     0, 0, 0, 1, 0, 0)
    vert2 = vertex(x + w, y,     0, 0, 0, 1, 1, 0)
    vert3 = vertex(x,     y + h, 0, 0, 0, 1, 0, 1)
    vert4 = vertex(x + w, y + h, 0.3, 0.3, 0.3, 1, 1, 1)
    #api.set_property_buffer(node_id, "verts", vert1 + vert2 + vert3 + vert4)
    #api.set_property_buffer(node_id, "faces", face(0, 2, 1) + face(1, 2, 3))

def draw():
    win_id = api.lookup_node_id("/window")

    # Add foo layer

    layer_id = api.add_node("foo", SceneNodeType.RENDER_LAYER)

    prop = Property(
        "is_visible", PropertyType.BOOL, PropertySubType.NULL,
        None,
        "Is Visible", "Visibility of the layer",
        False, False, 1, None, None, []
    )
    api.add_property(layer_id, prop)
    api.set_property_bool(layer_id, "is_visible", 0, True)

    #prop = Property(
    #    "redraw", PropertyType.BOOL, PropertySubType.NULL,
    #    None,
    #    "redraw", "Redraw this layer",
    #    False, False, 1, None, None, []
    #)
    #api.add_property(layer_id, prop)
    #api.set_property_bool(layer_id, "redraw", 0, True)

    prop = Property(
        "rect", PropertyType.UINT32, PropertySubType.PIXEL,
        None,
        "Rectangle", "The position and size within the layer",
        False, True, 4, None, None, []
    )
    api.add_property(layer_id, prop)
    # x
    api.set_property_u32(layer_id, "rect", 0, 0)
    # y
    api.set_property_u32(layer_id, "rect", 1, 0)
    # w
    #api.set_property_u32(layer_id, "rect", 2, int(3838/2))
    code = [["as_u32", ["/", ["load", "sw"], ["u32", 2]]]]
    code = [["as_u32", ["load", "sw"]]]
    api.set_property_expr(layer_id, "rect", 2, code)
    # h
    code = [["as_u32", ["/", ["load", "sh"], ["u32", 2]]]]
    code = [["as_u32", ["load", "sh"]]]
    api.set_property_expr(layer_id, "rect", 3, code)

    api.link_node(layer_id, win_id)

    # Add a bg box to our layer
    node_id = api.add_node("bg", SceneNodeType.RENDER_MESH)

    prop = Property(
        "data", PropertyType.BUFFER, PropertySubType.NULL,
        None,
        "Mesh Data", "The face and vertex data for the mesh",
        False, False, 2, None, None, []
    )
    api.add_property(node_id, prop)

    #x, y = 0.1, 0.1
    #w, h = 0.1, 0.1
    x, y, w, h = 0, 0, 1, 1
    #x, y, w, h = -1, 1, 2, -2
    vert1 = vertex(x,     y,     0, 0.1, 0, 1, 0, 0)
    vert2 = vertex(x + w, y,     0.1, 0, 0, 1, 1, 0)
    vert3 = vertex(x,     y + h, 0.1, 0, 0, 1, 0, 1)
    vert4 = vertex(x + w, y + h, 0.1, 0, 0, 1, 1, 1)

    verts = vert1 + vert2 + vert3 + vert4
    faces = face(0, 2, 1) + face(1, 2, 3)

    api.set_property_buf(node_id, "data", 0, verts)
    api.set_property_buf(node_id, "data", 1, faces)

    prop = Property(
        "rect", PropertyType.FLOAT32, PropertySubType.PIXEL,
        None,
        "Rectangle", "The position and size within the layer",
        False, True, 4, None, None, []
    )
    api.add_property(node_id, prop)
    # x
    api.set_property_f32(node_id, "rect", 0, 0)
    # y
    api.set_property_f32(node_id, "rect", 1, 0)
    # w
    #api.set_property_f32(node_id, "rect", 2, 20)
    code = [["-", ["load", "lw"], ["f32", 0]]]
    api.set_property_expr(node_id, "rect", 2, code)
    # h
    #api.set_property_str(node_id, "rect", 3, "lh - 10")
    code = [["-", ["load", "lh"], ["f32", 0]]]
    api.set_property_expr(node_id, "rect", 3, code)

    prop = Property(
        "z_index", PropertyType.UINT32, PropertySubType.NULL,
        None,
        "Z-index", "Z-index: values greater than zero are deferred draws",
        False, False, 1, None, None, []
    )
    api.add_property(node_id, prop)

    api.link_node(node_id, layer_id)

    # Add a second mesh to our layer

    node_id = api.add_node("meshie2", SceneNodeType.RENDER_MESH)

    prop = Property(
        "data", PropertyType.BUFFER, PropertySubType.NULL,
        None,
        "Mesh Data", "The face and vertex data for the mesh",
        False, False, 2, None, None, []
    )
    api.add_property(node_id, prop)

    x, y, w, h = 0, 0, 1, 1
    vert1 = vertex(x,     y,     1, 0, 1, 1, 0, 0)
    vert2 = vertex(x + w, y,     0.5, 0, 1, 1, 1, 0)
    vert3 = vertex(x,     y + h, 1, 0, 0.5, 1, 0, 1)
    vert4 = vertex(x + w, y + h, 0.5, 1, 0.5, 1, 1, 1)

    verts = vert1 + vert2 + vert3 + vert4
    faces = face(0, 2, 1) + face(1, 2, 3)

    api.set_property_buf(node_id, "data", 0, verts)
    api.set_property_buf(node_id, "data", 1, faces)

    prop = Property(
        "rect", PropertyType.FLOAT32, PropertySubType.PIXEL,
        None,
        "Rectangle", "The position and size within the layer",
        False, True, 4, None, None, []
    )
    api.add_property(node_id, prop)
    api.set_property_f32(node_id, "rect", 0, 10)
    api.set_property_f32(node_id, "rect", 1, 10)
    api.set_property_f32(node_id, "rect", 2, 60)
    api.set_property_f32(node_id, "rect", 3, 60)

    prop = Property(
        "z_index", PropertyType.UINT32, PropertySubType.NULL,
        None,
        "Z-index", "Z-index: values greater than zero are deferred draws",
        False, False, 1, None, None, []
    )
    api.add_property(node_id, prop)

    api.link_node(node_id, layer_id)

    # Add some text

    node_id = api.add_node("hellowurld", SceneNodeType.RENDER_TEXT)

    prop = Property(
        "rect", PropertyType.FLOAT32, PropertySubType.PIXEL,
        None,
        "Rectangle", "The position and size within the layer",
        False, True, 4, None, None, []
    )
    api.add_property(node_id, prop)
    api.set_property_f32(node_id, "rect", 0, 10)
    api.set_property_f32(node_id, "rect", 1, 100)
    api.set_property_f32(node_id, "rect", 2, 60)
    api.set_property_f32(node_id, "rect", 3, 60)

    prop = Property(
        "baseline", PropertyType.FLOAT32, PropertySubType.PIXEL,
        None,
        "Baseline", "Y offset of baseline inside rect",
        False, True, 1, None, None, []
    )
    api.add_property(node_id, prop)
    api.set_property_f32(node_id, "baseline", 0, 50)

    prop = Property(
        "font_size", PropertyType.FLOAT32, PropertySubType.PIXEL,
        None,
        "Font Size", "Font Size",
        False, True, 1, 0.0, None, []
    )
    api.add_property(node_id, prop)
    api.set_property_f32(node_id, "font_size", 0, 30)

    prop = Property(
        "text", PropertyType.STR, PropertySubType.NULL,
        None,
        "Text", "Text",
        False, False, 1, None, None, []
    )
    api.add_property(node_id, prop)
    api.set_property_str(node_id, "text", 0, "hello!😁🍆jelly 🍆1234")

    prop = Property(
        "color", PropertyType.FLOAT32, PropertySubType.COLOR,
        None,
        "Color", "Color of the text",
        False, False, 4, 0, 1, []
    )
    api.add_property(node_id, prop)
    api.set_property_f32(node_id, "color", 0, 1)
    api.set_property_f32(node_id, "color", 1, 1)
    api.set_property_f32(node_id, "color", 2, 1)
    api.set_property_f32(node_id, "color", 3, 1)

    #prop = Property(
    #    "overflow", PropertyType.ENUM, PropertySubType.NULL,
    #    None,
    #    "Overflow Behaviour", "Behaviour when text exceeds bounding box",
    #    False, True, 1, None, None, [
    #        "ScrollRight",
    #        "OverflowRight"
    #    ]
    #)
    #api.add_property(node_id, prop)
    #api.set_property_enum(node_id, "overflow", 0, "ScrollRight")

    prop = Property(
        "z_index", PropertyType.UINT32, PropertySubType.NULL,
        None,
        "Z-index", "Z-index: values greater than zero are deferred draws",
        False, False, 1, None, None, []
    )
    api.add_property(node_id, prop)

    prop = Property(
        "debug", PropertyType.BOOL, PropertySubType.NULL,
        None,
        "Debug", "Draw debug outlines",
        False, False, 1, None, None, []
    )
    api.add_property(node_id, prop)

    api.link_node(node_id, layer_id)

    # EditBox

    node_id = api.add_node("editz", SceneNodeType.EDIT_BOX)

    prop = Property(
        "is_active", PropertyType.BOOL, PropertySubType.NULL,
        None,
        "Is Active", "Whether the editbox is active",
        False, True, 1, None, None, []
    )
    api.add_property(node_id, prop)
    api.set_property_bool(node_id, "is_active", 0, True)

    prop = Property(
        "rect", PropertyType.FLOAT32, PropertySubType.PIXEL,
        None,
        "Rectangle", "The position and size within the layer",
        False, True, 4, None, None, []
    )
    api.add_property(node_id, prop)
    api.set_property_f32(node_id, "rect", 0, 60)
    api.set_property_f32(node_id, "rect", 1, 200)
    api.set_property_f32(node_id, "rect", 2, 300)
    api.set_property_f32(node_id, "rect", 3, 60)

    prop = Property(
        "baseline", PropertyType.FLOAT32, PropertySubType.PIXEL,
        None,
        "Baseline", "Y offset of baseline inside rect",
        False, True, 1, None, None, []
    )
    api.add_property(node_id, prop)
    api.set_property_f32(node_id, "baseline", 0, 50)

    prop = Property(
        "scroll", PropertyType.FLOAT32, PropertySubType.NULL,
        None,
        "Scroll", "Current scroll",
        False, False, 1, None, None, []
    )
    api.add_property(node_id, prop)

    prop = Property(
        "cursor_pos", PropertyType.UINT32, PropertySubType.NULL,
        None,
        "Cursor Position", "Cursor position within the text",
        False, False, 1, None, None, []
    )
    api.add_property(node_id, prop)

    prop = Property(
        "font_size", PropertyType.FLOAT32, PropertySubType.PIXEL,
        None,
        "Font Size", "Font Size",
        False, True, 1, 0.0, None, []
    )
    api.add_property(node_id, prop)
    api.set_property_f32(node_id, "font_size", 0, 50)

    prop = Property(
        "text", PropertyType.STR, PropertySubType.NULL,
        None,
        "Text", "Text",
        False, False, 1, None, None, []
    )
    api.add_property(node_id, prop)
    api.set_property_str(node_id, "text", 0, "hello king!😁🍆jelly 🍆1234")

    prop = Property(
        "text_color", PropertyType.FLOAT32, PropertySubType.COLOR,
        None,
        "Text Color", "Color of the text",
        False, False, 4, 0, 1, []
    )
    api.add_property(node_id, prop)
    api.set_property_f32(node_id, "text_color", 0, 1)
    api.set_property_f32(node_id, "text_color", 1, 1)
    api.set_property_f32(node_id, "text_color", 2, 1)
    api.set_property_f32(node_id, "text_color", 3, 1)

    prop = Property(
        "cursor_color", PropertyType.FLOAT32, PropertySubType.COLOR,
        None,
        "Cursor Color", "Color of the cursor",
        False, False, 4, 0, 1, []
    )
    api.add_property(node_id, prop)
    api.set_property_f32(node_id, "cursor_color", 0, 1)
    api.set_property_f32(node_id, "cursor_color", 1, 0.5)
    api.set_property_f32(node_id, "cursor_color", 2, 0.5)
    api.set_property_f32(node_id, "cursor_color", 3, 1)

    prop = Property(
        "hi_bg_color", PropertyType.FLOAT32, PropertySubType.COLOR,
        None,
        "Highlight Bg Color", "Background color for highlighted text",
        False, False, 4, 0, 1, []
    )
    api.add_property(node_id, prop)
    api.set_property_f32(node_id, "hi_bg_color", 0, 1)
    api.set_property_f32(node_id, "hi_bg_color", 1, 1)
    api.set_property_f32(node_id, "hi_bg_color", 2, 1)
    api.set_property_f32(node_id, "hi_bg_color", 3, 0.5)

    prop = Property(
        "selected", PropertyType.UINT32, PropertySubType.NULL,
        None,
        "Selected", "Selected range",
        True, False, 2, 0, None, []
    )
    api.add_property(node_id, prop)
    api.set_property_u32(node_id, "selected", 0, 1)
    api.set_property_u32(node_id, "selected", 1, 4)

    prop = Property(
        "z_index", PropertyType.UINT32, PropertySubType.NULL,
        None,
        "Z-index", "Z-index: values greater than zero are deferred draws",
        False, False, 1, None, None, []
    )
    api.add_property(node_id, prop)
    api.set_property_u32(node_id, "z_index", 0, 4)

    prop = Property(
        "debug", PropertyType.BOOL, PropertySubType.NULL,
        None,
        "Debug", "Draw debug outlines",
        False, False, 1, None, None, []
    )
    api.add_property(node_id, prop)
    api.set_property_bool(node_id, "debug", 0, True)

    arg_data = bytearray()
    serial.write_u32(arg_data, node_id)
    api.call_method(win_id, "create_edit_box", arg_data)

    api.link_node(node_id, layer_id)

    # ChatView

    node_id = api.add_node("chatty", SceneNodeType.CHAT_VIEW)

    prop = Property(
        "rect", PropertyType.FLOAT32, PropertySubType.PIXEL,
        None,
        "Rectangle", "The position and size within the layer",
        False, True, 4, None, None, []
    )
    api.add_property(node_id, prop)
    api.set_property_f32(node_id, "rect", 0, 50)
    api.set_property_f32(node_id, "rect", 1, 260)
    code = [["-", ["load", "lw"], ["f32", 100]]]
    api.set_property_expr(node_id, "rect", 2, code)
    code = [["-", ["load", "lh"], ["f32", 260]]]
    api.set_property_expr(node_id, "rect", 3, code)

    prop = Property(
        "debug", PropertyType.BOOL, PropertySubType.NULL,
        None,
        "Debug", "Draw debug outlines",
        False, False, 1, None, None, []
    )
    api.add_property(node_id, prop)
    api.set_property_bool(node_id, "debug", 0, True)

    prop = Property(
        "z_index", PropertyType.UINT32, PropertySubType.NULL,
        None,
        "Z-index", "Z-index: values greater than zero are deferred draws",
        False, False, 1, None, None, []
    )
    api.add_property(node_id, prop)

    arg_data = bytearray()
    serial.write_u32(arg_data, node_id)
    api.call_method(win_id, "create_chat_view", arg_data)

    api.link_node(node_id, layer_id)

class App2(EventLoop):

    def key_down(self, keycode, keymods, repeat):
        if repeat:
            return

        win_id = api.lookup_node_id("/window")
        scale = api.get_property_value(win_id, "scale")[0]

        if keymods.ctrl and keycode == "=":
            scale *= 1.01
        elif keymods.ctrl and keycode == "-":
            scale *= 0.99

        print(scale)
        api.set_property_f32(win_id, "scale", 0, scale)

def main():
    draw()

    # DEBUG
    print_tree()

    app = App2()
    app.run()

#def main():
#    if True:
#        node_id = api.add_node("foo", SceneNodeType.WINDOW)
#        prop = Property(
#            "myprop", PropertyType.FLOAT32, PropertySubType.NULL,
#            None,
#            "myprop", "",
#            False, 2, None, None, []
#        )
#        api.add_property(1, prop)
#        api.link_node(node_id, 0)
#        api.set_property_f32(1, "myprop", 0, 4.0)
#        api.set_property_f32(1, "myprop", 1, 110.0)
#    print("val =", api.get_property_value(1, "myprop"))
#    for prop in api.get_properties(1):
#        print("Property:")
#        print(f"  name = {prop.name}")
#        print(f"  type = {prop.type}")
#        print(f"  subtype = {prop.subtype}")
#        print(f"  defaults = {prop.defaults}")
#        print(f"  ui_name = {prop.ui_name}")
#        print(f"  desc = {prop.desc}")
#        print(f"  is_null_allowed = {prop.is_null_allowed}")
#        print(f"  array_len = {prop.array_len}")
#        print(f"  min_val = {prop.min_val}")
#        print(f"  max_val = {prop.max_val}")
#        print(f"  enum_items = {prop.enum_items}")
#        print()
#    print_tree()
#    #garbage_collect()
#
#    #app = App()
#    #draw_box()
#    #draw_rounded_box()
#    #draw_cursor()
#    #reposition_cursor()
#    ##print_tree()
#    #app.run()

