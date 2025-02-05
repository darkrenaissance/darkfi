from pydrk import SceneNodeType, PropertyType
from .api import api

def join(parent_path, child_name):
    if parent_path == "/":
        return f"/{child_name}"
    return f"{parent_path}/{child_name}"

def print_tree(node_path="/", depth=None):
    print(node_path)
    print_node_info(node_path, depth, indent=1)

def print_node_info(parent_path, depth, indent):
    if indent - 1 == depth:
        return

    ws = " "*4*indent
    for (child_name, child_id, child_type) in api.get_children(parent_path):
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
            case SceneNodeType.LAYER:
                child_type = "layer"
            case SceneNodeType.OBJECT:
                child_type = "object"
            case SceneNodeType.VECTOR_ART:
                child_type = "vector_art"
            case SceneNodeType.TEXT:
                child_type = "text"
            case SceneNodeType.TEXTURE:
                child_type = "texture"
            case SceneNodeType.FONTS:
                child_type = "fonts"
            case SceneNodeType.FONT:
                child_type = "font"
            case SceneNodeType.CHAT_VIEW:
                child_type = "chat_view"
            case SceneNodeType.EDIT_BOX:
                child_type = "edit_box"
            case SceneNodeType.CHAT_EDIT:
                child_type = "chat_edit"
            case SceneNodeType.BUTTON:
                child_type = "button"
            case SceneNodeType.SETTING_ROOT:
                child_type = "setting_root"
            case SceneNodeType.SETTING:
                child_type = "setting"

        desc = f"{ws}{child_name}:{child_id}/"
        desc += " "*(50 - len(desc))
        desc += f"[{child_type}]"
        print(desc)

        if parent_path == "/":
            child_path = "/" + child_name
        else:
            child_path = parent_path + "/" + child_name

        print_node_info(child_path, depth, indent+1)

    for prop in api.get_properties(parent_path):
        if prop.type != PropertyType.BUFFER:
            prop_val = api.get_property_value(parent_path, prop.name)

            if prop.type == PropertyType.STR:
                prop_val = [f"\"{pv}\"" for pv in prop_val]

            if len(prop_val) == 1:
                prop_val = prop_val[0]

            prop_val = f" = {prop_val}"
        else:
            prop_val = ""

        prop_type = PropertyType.to_str(prop.type)

        print(f"{ws}{prop.name}: {prop_type}{prop_val}")
        if prop.depends:
            print(f"{ws}    depends: {prop.depends}")

    #for sig in api.get_signals(parent_id):
    #    print(f"{ws}~{sig}")
    #    for slot_id, slot in api.get_slots(parent_id, sig):
    #        print(f"{ws}- '{slot}' ({slot_id})")

    #for method_name in api.get_methods(parent_id):
    #    args, results = api.get_method(parent_id, method_name)

    #    args = [f"{name}: " + PropertyType.to_str(typ) for (name, _, typ) in args]
    #    results = [f"{name}: " + PropertyType.to_str(typ) for (name, _, typ) in results]

    #    method_str = f"{method_name}(" + ", ".join(args) + ") -> (" + ", ".join(results) + ")"
    #    print(f"{ws}{method_str}")

