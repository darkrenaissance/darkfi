from pydrk import SceneNodeType, PropertyType, vertex, face
from .api import *

def clear_layer(layer_name):
    layer_id = api.lookup_node_id(f"/window/{layer_name}")
    if layer_id is None:
        return

    unlink_node(layer_id, "/window")

    for (_, child_id, child_type) in api.get_children(layer_id):
        api.unlink_node(child_id, layer_id)
        if child_type == SceneNodeType.RENDER_OBJECT:
            remove_node_recursive(child_id)

    api.remove_node(layer_id)

def add_layer(layer_name):
    layer_id = api.add_node(layer_name, SceneNodeType.RENDER_LAYER)
    add_property_u32(layer_id, "rect_x")
    add_property_u32(layer_id, "rect_y")
    add_property_u32(layer_id, "rect_w")
    add_property_u32(layer_id, "rect_h")
    add_property_bool(layer_id, "is_visible", True)
    link_node(layer_id, "/window")
    return layer_id

class Layer:

    def __init__(self, name):
        self.name = name
        clear_layer(name)
        self.id = add_layer(name)

    def resize(self, w, h):
        set_property_u32(self.id, "rect_w", w)
        set_property_u32(self.id, "rect_h", h)

    def add_obj(self, name):
        obj_id = api.add_node(name, SceneNodeType.RENDER_OBJECT)
        add_property_f32(obj_id, "x")
        add_property_f32(obj_id, "y")
        add_property_f32(obj_id, "scale", 1.0)
        link_node(obj_id, self.id)

