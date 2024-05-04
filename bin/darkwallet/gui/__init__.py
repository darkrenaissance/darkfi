from .print_tree import print_tree
from .api import *
from .gfx import Layer
from . import settings

class App(EventLoop):

    def __init__(self):
        super().__init__()

        w = get_property("/window", "width")
        h = get_property("/window", "height")

        self.chatbox_layer = Layer("chatbox_layer")
        self.chatbox_layer.resize(w, h)

        self.chatbox_layer.add_obj("outline")

    def resize_event(self, w, h):
        self.chatbox_layer.resize(w, h)
        resize_box()

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

def draw_box():
    from pydrk import SceneNodeType, PropertyType

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
    from pydrk import vertex, face

    mesh_id = api.lookup_node_id("/window/chatbox_layer/outline/box")

    layer_w = get_property("/window/chatbox_layer", "rect_w")
    layer_h = get_property("/window/chatbox_layer", "rect_h")

    # Lets add a poly - must be counterclockwise
    x, y = layer_w * 0.25, layer_h * 0.25
    w, h = layer_w * 0.50, layer_h * 0.50
    vert1 = vertex(x,     y,     1, 1, 1, 1, 0, 0)
    vert2 = vertex(x + w, y,     1, 1, 1, 1, 1, 0)
    vert3 = vertex(x,     y + h, 1, 1, 1, 1, 0, 1)
    vert4 = vertex(x + w, y + h, 1, 1, 1, 1, 1, 1)
    api.set_property_buffer(mesh_id, "verts", vert1 + vert2 + vert3 + vert4)
    api.set_property_buffer(mesh_id, "faces", face(0, 2, 1) + face(1, 2, 3))

    # Second mesh
    mesh_id = api.lookup_node_id("/window/chatbox_layer/outline/inner_box")

    x, y = x + 5, y + 5
    w -= 10
    h -= 10
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
    print_tree()
    app.run()

