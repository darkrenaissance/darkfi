#!/usr/bin/python
# Ping a peer
from pydrk import Api, exc, ErrorCode, HostApi, SceneNodeType, PropertyType, vertex, face, serial
import sys, zmq

def join(parent_path, child_name):
    if parent_path == "/":
        return f"/{child_name}"
    return f"{parent_path}/{child_name}"

def print_tree():
    root_id = api.lookup_node_id("/")
    print_node_info(root_id)

def print_node_info(parent_id, indent=0):
    ws = " "*4*indent
    for (child_name, child_id, child_type) in api.get_children(parent_id):
        match child_type:
            case SceneNodeType.ROOT:
                child_type = "root"
            case SceneNodeType.WINDOW:
                child_type = "window"
            case SceneNodeType.WINDOW_INPUT:
                child_type = "window_input"
            case SceneNodeType.KEYBOARD:
                child_type = "keyboard"
            case SceneNodeType.MOUSE:
                child_type = "mouse"
            case SceneNodeType.RENDER_LAYER:
                child_type = "render_layer"
            case SceneNodeType.RENDER_OBJECT:
                child_type = "render_object"
            case SceneNodeType.RENDER_MESH:
                child_type = "render_mesh"
            case SceneNodeType.RENDER_TEXT:
                child_type = "render_text"
            case SceneNodeType.RENDER_TEXTURE:
                child_type = "render_texture"
            case SceneNodeType.FONTS:
                child_type = "fonts"
            case SceneNodeType.FONT:
                child_type = "font"
            case SceneNodeType.LINE_POSITION:
                child_type = "line_position"

        desc = f"{ws}{child_name}:{child_id}/"
        desc += " "*(50 - len(desc))
        desc += f"[{child_type}]"
        print(desc)

        print_node_info(child_id, indent+1)

    for prop_name, prop_type in api.get_properties(parent_id):
        if prop_type == PropertyType.STR:
            prop_val = api.get_property(parent_id, prop_name)
            prop_val = f" = \"{prop_val}\""
        elif prop_type != PropertyType.BUFFER:
            prop_val = api.get_property(parent_id, prop_name)
            prop_val = f" = {prop_val}"
        else:
            prop_val = ""

        prop_type = PropertyType.to_str(prop_type)

        print(f"{ws}{prop_name}: {prop_type}{prop_val}")

    for sig in api.get_signals(parent_id):
        print(f"{ws}~{sig}")
        for slot_id, slot in api.get_slots(parent_id, sig):
            print(f"{ws}- '{slot}' ({slot_id})")

    for method_name in api.get_methods(parent_id):
        args, results = api.get_method(parent_id, method_name)

        args = [f"{name}: " + PropertyType.to_str(typ) for (name, typ) in args]
        results = [f"{name}: " + PropertyType.to_str(typ) for (name, typ) in results]

        method_str = f"{method_name}(" + ", ".join(args) + ") -> (" + ", ".join(results) + ")"
        print(f"{ws}{method_str}")

def make_sub_socket():
    context = zmq.Context()
    socket = context.socket(zmq.SUB)
    socket.setsockopt(zmq.SUBSCRIBE, b'')
    socket.connect("tcp://localhost:9485")
    return socket

def remove_node_recursive(node_id):
    for (_, child_id, _) in api.get_children(node_id):
        # Unlink the child
        api.unlink_node(child_id, node_id)
        # Remove the node
        remove_node_recursive(child_id)
    # Garbage collection
    if not api.get_parents(node_id):
        api.remove_node(node_id)

def clear_layer(layer_path):
    layer_id = api.lookup_node_id("/window/layer2")
    if layer_id is None:
        return

    win_id = api.lookup_node_id("/window")
    api.unlink_node(layer_id, win_id)

    for (_, child_id, child_type) in api.get_children(layer_id):
        api.unlink_node(child_id, layer_id)
        if child_type == SceneNodeType.RENDER_OBJECT:
            remove_node_recursive(child_id)

    api.remove_node(layer_id)

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

def set_property(node_path, prop, val):
    node_id = api.lookup_node_id(node_path)
    match val:
        case float():
            api.set_property_f32(node_id, prop, val)
        case int():
            api.set_property_u32(node_id, prop, val)

def recalc_areas():
    print("recalc areas")
    w = get_property("/window", "width")
    h = get_property("/window", "height")
    set_property("/window/layer2", "rect_w", int(w))
    set_property("/window/layer2", "rect_h", int(h))

    print_tree()

def garbage_collect():
    for node_id in api.scan_dangling():
        remove_node_recursive(node_id)

api = Api()
host = HostApi(api)
print(api.hello())

garbage_collect()
clear_layer("/window/layer2")

print("Generating scene...")
subsock = make_sub_socket()

register_slot("/window",                "resize",        b"resize")
register_slot("/window/input/mouse",    "button_down",   b"click")
register_slot("/window/input/mouse",    "wheel",         b"wheel")
#register_slot("/window/input/mouse",    "move",          b"mouse")

layer_id = api.add_node("layer2", SceneNodeType.RENDER_LAYER)
api.add_property(layer_id, "rect_x", PropertyType.UINT32)
api.add_property(layer_id, "rect_y", PropertyType.UINT32)
api.add_property(layer_id, "rect_w", PropertyType.UINT32)
api.add_property(layer_id, "rect_h", PropertyType.UINT32)
api.add_property(layer_id, "is_visible", PropertyType.BOOL)
api.set_property_bool(layer_id, "is_visible", True)
win_id = api.lookup_node_id("/window")
api.link_node(layer_id, win_id)

if True:
    layer_id = api.lookup_node_id("/window/layer2")

    obj_id = api.add_node("obj1", SceneNodeType.RENDER_OBJECT)
    api.add_property(obj_id, "x", PropertyType.FLOAT32)
    api.add_property(obj_id, "y", PropertyType.FLOAT32)
    api.add_property(obj_id, "scale", PropertyType.FLOAT32)
    api.set_property_f32(obj_id, "scale", 1.0)

    mesh_id = api.add_node("mesh", SceneNodeType.RENDER_MESH)
    api.add_property(mesh_id, "verts", PropertyType.BUFFER)
    api.add_property(mesh_id, "faces", PropertyType.BUFFER)

    texture_id = host.load_texture("da_king", "king.png")
    api.link_node(texture_id, obj_id)
    # To preserve the ratio, we should do this when resize happens too
    texture_w = api.get_property(texture_id, "width")
    texture_h = api.get_property(texture_id, "width")
    texture_ratio = texture_w / texture_h
    screen_w = get_property("/window", "width")
    screen_h = get_property("/window", "height")
    screen_ratio = screen_w / screen_h
    aspect_ratio = texture_ratio / screen_ratio

    #api.set_property_bool("/window/layer2", "is_visible", False)
    # Lets add a poly - must be counterclockwise
    x, y = 0.25, 0.25
    w, h = 0.5 * aspect_ratio, 0.5
    vert1 = vertex(x,     y,     1, 1, 1, 1, 0, 0)
    vert2 = vertex(x + w, y,     1, 1, 1, 1, 1, 0)
    vert3 = vertex(x,     y + h, 1, 1, 1, 1, 0, 1)
    vert4 = vertex(x + w, y + h, 1, 1, 1, 1, 1, 1)
    api.set_property_buffer(mesh_id, "verts", vert1 + vert2 + vert3 + vert4)
    api.set_property_buffer(mesh_id, "faces", face(0, 2, 1) + face(1, 2, 3))

    api.link_node(mesh_id, obj_id)
    api.link_node(obj_id, layer_id)

if True:
    layer_id = api.lookup_node_id("/window/layer2")
    obj_id = api.add_node("obj", SceneNodeType.RENDER_OBJECT)
    api.add_property(obj_id, "x", PropertyType.FLOAT32)
    api.add_property(obj_id, "y", PropertyType.FLOAT32)
    api.add_property(obj_id, "scale", PropertyType.FLOAT32)
    api.set_property_f32(obj_id, "scale", 1.0)

    # Call method
    text_id = host.create_text(
        "/font/inter-regular", "mytxt2", "hello world", 34
    )
    if text_id is None:
        sys.exit(-1)
    print("create text node id:", text_id)

    api.set_property_f32(obj_id, "x", 0.5)
    api.set_property_f32(text_id, "r", 1.0)
    api.set_property_f32(text_id, "g", 1.0)
    api.set_property_f32(text_id, "b", 1.0)
    api.set_property_f32(text_id, "a", 1.0)

    api.link_node(text_id, obj_id)
    api.link_node(obj_id, layer_id)

# Screen init
recalc_areas()

print_tree()
print()
print("Scene created.")

while True:
    data = subsock.recv()
    match data:
        case b"resize":
            recalc_areas()
        case b"click":
            x = get_property("/window/input/mouse", "x")
            y = get_property("/window/input/mouse", "y")
            print(f"mouse clicked ({x}, {y})")
        case b"wheel":
            print("mouse wheely")

