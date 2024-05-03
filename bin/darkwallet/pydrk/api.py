import zmq
from . import serial
from . import exc

class Command:
    HELLO = 0
    ADD_NODE = 1
    REMOVE_NODE = 9
    RENAME_NODE = 23,
    LOOKUP_NODE_ID = 12
    ADD_PROPERTY = 11
    LINK_NODE = 2
    UNLINK_NODE = 8
    GET_INFO = 19
    GET_CHILDREN = 4
    GET_PARENTS = 5
    GET_PROPERTIES = 3
    GET_PROPERTY = 6
    SET_PROPERTY = 7
    GET_SIGNALS = 14
    REGISTER_SLOT = 15
    UNREGISTER_SLOT = 16
    LOOKUP_SLOT_ID = 17
    GET_SLOTS = 18
    GET_METHODS = 20
    GET_METHOD = 21
    CALL_METHOD = 22

class SceneNodeType:
    NULL = 0
    ROOT = 1
    WINDOW = 2
    WINDOW_INPUT = 6
    KEYBOARD = 7
    MOUSE = 8
    RENDER_LAYER = 3
    RENDER_OBJECT = 4
    RENDER_MESH = 5
    RENDER_TEXT = 9
    RENDER_TEXTURE = 13
    FONTS = 10
    FONT = 11
    LINE_POSITION = 12

class PropertyType:
    NULL = 0
    BUFFER = 1
    BOOL = 2
    UINT32 = 3
    FLOAT32 = 4
    STR = 5
    SCENE_NODE_ID = 6

    @staticmethod
    def to_str(prop_type):
        match prop_type:
            case PropertyType.NULL:
                return "null"
            case PropertyType.BUFFER:
                return "buffer"
            case PropertyType.BOOL:
                return "bool"
            case PropertyType.UINT32:
                return "uint32"
            case PropertyType.FLOAT32:
                return "float32"
            case PropertyType.STR:
                return "str"
            case PropertyType.SCENE_NODE_ID:
                return "scene_node_id"

class ErrorCode:
    INVALID_SCENE_PATH = 2
    NODE_NOT_FOUND = 3
    CHILD_NODE_NOT_FOUND = 4
    PARENT_NODE_NOT_FOUND = 5
    PROPERTY_ALREADY_EXISTS = 6
    PROPERTY_NOT_FOUND = 7
    PROPERTY_WRONG_TYPE = 8
    SIGNAL_ALREADY_EXISTS = 9
    SIGNAL_NOT_FOUND = 10
    SLOT_NOT_FOUND = 11
    METHOD_NOT_FOUND = 12
    NODES_ARE_LINKED = 13
    NODES_NOT_LINKED = 14
    NODE_HAS_PARENTS = 15
    NODE_HAS_CHILDREN = 16
    NODE_PARENT_NAME_CONFLICT = 17
    NODE_CHILD_NAME_CONFLICT = 18
    NODE_SIBLING_NAME_CONFLICT = 19
    FILE_NOT_FOUND = 20

    @staticmethod
    def to_str(errc):
        match errc:
            case ErrorCode.INVALID_SCENE_PATH:
                return "invalid_scene_path"
            case ErrorCode.NODE_NOT_FOUND:
                return "node_not_found"
            case ErrorCode.CHILD_NODE_NOT_FOUND:
                return "child_node_not_found"
            case ErrorCode.PARENT_NODE_NOT_FOUND:
                return "parent_node_not_found"
            case ErrorCode.PROPERTY_ALREADY_EXISTS:
                return "property_already_exists"
            case ErrorCode.PROPERTY_NOT_FOUND:
                return "property_not_found"
            case ErrorCode.PROPERTY_WRONG_TYPE:
                return "property_wrong_type"
            case ErrorCode.SIGNAL_ALREADY_EXISTS:
                return "signal_already_exists"
            case ErrorCode.SIGNAL_NOT_FOUND:
                return "signal_not_found "
            case ErrorCode.SLOT_NOT_FOUND:
                return "slot_not_found "
            case ErrorCode.METHOD_NOT_FOUND:
                return "method_not_found "
            case ErrorCode.NODES_ARE_LINKED:
                return "nodes_are_linked "
            case ErrorCode.NODES_NOT_LINKED:
                return "nodes_not_linked "
            case ErrorCode.NODE_HAS_PARENTS:
                return "node_has_parents "
            case ErrorCode.NODE_HAS_CHILDREN:
                return "node_has_children "
            case ErrorCode.NODE_PARENT_NAME_CONFLICT:
                return "node_parent_name_conflict "
            case ErrorCode.NODE_CHILD_NAME_CONFLICT:
                return "node_child_name_conflict "
            case ErrorCode.NODE_SIBLING_NAME_CONFLICT:
                return "node_sibling_name_conflict "
            case ErrorCode.FILE_NOT_FOUND:
                return "file_not_found"

def vertex(x, y, r, g, b, a, u, v):
    buf = bytearray()
    serial.write_f32(buf, x)
    serial.write_f32(buf, y)
    serial.write_f32(buf, r)
    serial.write_f32(buf, g)
    serial.write_f32(buf, b)
    serial.write_f32(buf, a)
    serial.write_f32(buf, u)
    serial.write_f32(buf, v)
    return buf

def face(idx1, idx2, idx3):
    buf = bytearray()
    serial.write_u32(buf, idx1)
    serial.write_u32(buf, idx2)
    serial.write_u32(buf, idx3)
    return buf

class Api:

    def __init__(self, addr="[::1]", port=9484):
        context = zmq.Context()
        self.socket = context.socket(zmq.REQ)
        self.socket.setsockopt(zmq.IPV6, True)
        self.socket.connect(f"tcp://{addr}:{port}")

    def _make_request(self, cmd, payload):
        req_cmd = bytearray()
        serial.write_u8(req_cmd, cmd)
        self.socket.send_multipart([req_cmd, payload])

        errc, reply = self.socket.recv_multipart()
        errc = int.from_bytes(errc, "little")
        cursor = serial.Cursor(reply)
        match errc:
            case 2:
                raise exc.RequestInvalidScenePath
            case 3:
                raise exc.RequestNodeNotFound
            case 4:
                raise exc.RequestChildNodeNotFound
            case 5:
                raise exc.RequestParentNodeNotFound
            case 6:
                raise exc.RequestPropertyAlreadyExists
            case 7:
                raise exc.RequestPropertyNotFound
            case 8:
                raise exc.RequestPropertyWrongType
            case 9:
                raise exc.RequestSignalAlreadyExists
            case 10:
                raise exc.RequestSignalNotFound
            case 11:
                raise exc.RequestSlotNotFound
            case 12:
                raise exc.RequestMethodNotFound
            case 13:
                raise exc.RequestNodesAreLinked
            case 14:
                raise exc.RequestNodesNotLinked
            case 15:
                raise exc.RequestNodeHasParents
            case 16:
                raise exc.RequestNodeHasChildren
            case 17:
                raise exc.RequestNodeParentNameConflict
            case 18:
                raise exc.RequestNodeChildNameConflict
            case 19:
                raise exc.RequestNodeSiblingNameConflict
            case 20:
                raise exc.RequestFileNotFound
        return cursor

    def hello(self):
        response = self._make_request(Command.HELLO, bytearray())
        return serial.decode_str(response)

    def get_info(self, node_id):
        req = bytearray()
        serial.write_u32(req, node_id)
        cur = self._make_request(Command.GET_INFO, req)
        name = serial.decode_str(cur)
        type = serial.read_u8(cur)
        return (name, type)

    def get_children(self, node_id):
        req = bytearray()
        serial.write_u32(req, node_id)
        cur = self._make_request(Command.GET_CHILDREN, req)
        children_len = serial.decode_varint(cur)
        children = []
        for _ in range(children_len):
            child_name = serial.decode_str(cur)
            child_id = serial.read_u32(cur)
            child_type = serial.read_u8(cur)
            children.append((child_name, child_id, child_type))
        return children

    def get_parents(self, node_id):
        req = bytearray()
        serial.write_u32(req, node_id)
        cur = self._make_request(Command.GET_PARENTS, req)
        parents_len = serial.decode_varint(cur)
        parents = []
        for _ in range(parents_len):
            parent_name = serial.decode_str(cur)
            parent_id = serial.read_u32(cur)
            parent_type = serial.read_u8(cur)
            parents.append((parent_name, parent_id, parent_type))
        return parents

    def get_properties(self, node_id):
        req = bytearray()
        serial.write_u32(req, node_id)
        cur = self._make_request(Command.GET_PROPERTIES, req)
        props_len = serial.decode_varint(cur)
        props = []
        for _ in range(props_len):
            prop_name = serial.decode_str(cur)
            prop_type = serial.read_u8(cur)
            props.append((prop_name, prop_type))
        return props

    def get_property(self, node_id, prop_name):
        req = bytearray()
        serial.write_u32(req, node_id)
        serial.encode_str(req, prop_name)
        cur = self._make_request(Command.GET_PROPERTY, req)
        prop_type = serial.read_u8(cur)
        match prop_type:
            case 0:
                return None
            case 1:
                return []
            case 3:
                val = serial.read_u8(cur)
                return bool(val)
            case 2:
                return serial.read_u32(cur)
            case 4:
                return serial.read_f32(cur)
            case 5:
                return serial.decode_str(cur)
        raise Exception("unknown property type returned")

    def add_node(self, node_name, node_type):
        req = bytearray()
        serial.encode_str(req, node_name)
        serial.write_u8(req, int(node_type))
        cur = self._make_request(Command.ADD_NODE, req)
        node_id = serial.read_u32(cur)
        return node_id

    def remove_node(self, node_id):
        req = bytearray()
        serial.write_u32(req, node_id)
        self._make_request(Command.REMOVE_NODE, req)

    def rename_node(self, node_id, node_name):
        req = bytearray()
        serial.write_u32(req, node_id)
        serial.encode_str(req, node_name)
        self._make_request(Command.RENAME_NODE, req)

    def lookup_node_id(self, node_path):
        req = bytearray()
        serial.encode_str(req, node_path)
        try:
            cur = self._make_request(Command.LOOKUP_NODE_ID, req)
        except exc.RequestNodeNotFound:
            return None
        return serial.read_u32(cur)

    def add_property(self, node_id, prop_name, prop_type):
        req = bytearray()
        serial.write_u32(req, node_id)
        serial.encode_str(req, prop_name)
        serial.write_u8(req, int(prop_type))
        self._make_request(Command.ADD_PROPERTY, req)

    def link_node(self, child_id, parent_id):
        req = bytearray()
        serial.write_u32(req, child_id)
        serial.write_u32(req, parent_id)
        self._make_request(Command.LINK_NODE, req)

    def unlink_node(self, child_id, parent_id):
        req = bytearray()
        serial.write_u32(req, child_id)
        serial.write_u32(req, parent_id)
        self._make_request(Command.UNLINK_NODE, req)

    def set_property_bool(self, node_id, prop_name, val):
        req = bytearray()
        serial.write_u32(req, node_id)
        serial.encode_str(req, prop_name)
        serial.write_u8(req, int(val))
        self._make_request(Command.SET_PROPERTY, req)

    def set_property_u32(self, node_id, prop_name, val):
        req = bytearray()
        serial.write_u32(req, node_id)
        serial.encode_str(req, prop_name)
        serial.write_u32(req, val)
        self._make_request(Command.SET_PROPERTY, req)

    def set_property_f32(self, node_id, prop_name, val):
        req = bytearray()
        serial.write_u32(req, node_id)
        serial.encode_str(req, prop_name)
        serial.write_f32(req, val)
        self._make_request(Command.SET_PROPERTY, req)

    def set_property_buffer(self, node_id, prop_name, buf):
        req = bytearray()
        serial.write_u32(req, node_id)
        serial.encode_str(req, prop_name)
        serial.encode_buf(req, buf)
        self._make_request(Command.SET_PROPERTY, req)

    def set_property_str(self, node_id, prop_name, val):
        req = bytearray()
        serial.write_u32(req, node_id)
        serial.encode_str(req, prop_name)
        serial.encode_str(req, val)
        self._make_request(Command.SET_PROPERTY, req)

    def get_signals(self, node_id):
        req = bytearray()
        serial.write_u32(req, node_id)
        cur = self._make_request(Command.GET_SIGNALS, req)
        sigs_len = serial.decode_varint(cur)
        sigs = []
        for _ in range(sigs_len):
            sigs.append(serial.decode_str(cur))
        return sigs

    def register_slot(self, node_id, sig_name, slot_name, user_data):
        req = bytearray()
        serial.write_u32(req, node_id)
        serial.encode_str(req, sig_name)
        serial.encode_str(req, slot_name)
        serial.encode_varint(req, len(user_data))
        req += user_data
        cur = self._make_request(Command.REGISTER_SLOT, req)
        slot_id = serial.read_u32(cur)
        return slot_id

    def unregister_slot(self, node_id, sig_name, slot_id):
        req = bytearray()
        serial.write_u32(req, node_id)
        serial.encode_str(req, sig_name)
        serial.write_u32(req, slot_id)
        self._make_request(Command.UNREGISTER_SLOT, req)

    def lookup_slot_id(self, node_id, sig_name, slot_name):
        req = bytearray()
        serial.write_u32(req, node_id)
        serial.encode_str(req, sig_name)
        serial.encode_str(req, slot_name)
        try:
            cur = self._make_request(Command.LOOKUP_SLOT_ID, req)
        except exc.RequestSlotNotFound:
            return None
        return serial.read_u32(cur)

    def get_slots(self, node_id, sig_name):
        req = bytearray()
        serial.write_u32(req, node_id)
        serial.encode_str(req, sig_name)
        cur = self._make_request(Command.GET_SLOTS, req)
        slots_len = serial.decode_varint(cur)
        slots = []
        for _ in range(slots_len):
            slot_name = serial.decode_str(cur)
            slot_id = serial.read_u32(cur)
            slots.append((slot_id, slot_name))
        return slots

    def get_methods(self, node_id):
        req = bytearray()
        serial.write_u32(req, node_id)
        cur = self._make_request(Command.GET_METHODS, req)
        methods_len = serial.decode_varint(cur)
        methods = []
        for _ in range(methods_len):
            method_name = serial.decode_str(cur)
            methods.append(method_name)
        return methods

    def get_method(self, node_id, method_name):
        req = bytearray()
        serial.write_u32(req, node_id)
        serial.encode_str(req, method_name)
        cur = self._make_request(Command.GET_METHOD, req)
        args_len = serial.decode_varint(cur)
        args = []
        for _ in range(args_len):
            arg_name = serial.decode_str(cur)
            arg_type = serial.read_u8(cur)
            args.append((arg_name, arg_type))
        results_len = serial.decode_varint(cur)
        results = []
        for _ in range(results_len):
            result_name = serial.decode_str(cur)
            result_type = serial.read_u8(cur)
            results.append((result_name, result_type))
        return (args, results)

    def call_method(self, node_id, method_name, arg_data):
        req = bytearray()
        serial.write_u32(req, node_id)
        serial.encode_str(req, method_name)
        serial.encode_buf(req, arg_data)
        cur = self._make_request(Command.CALL_METHOD, req)
        errc = serial.read_u8(cur)
        result = serial.decode_buf(cur)
        return (errc, result)

