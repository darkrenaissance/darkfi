import zmq
from collections import namedtuple
from . import serial, exc

class Expr(str):
    """String rendering of an expr-bound property value, sent over netdebug.
    Subclasses str so it formats naturally, but stays isinstance-distinct
    from plain str property values."""
    pass

Property = namedtuple("Property", [
    "name",
    "type",
    "subtype",
    #"defaults",
    "ui_name",
    "desc",
    "is_null_allowed",
    "is_expr_allowed",
    "array_len",
    "min_val",
    "max_val",
    "enum_items",
    "depends"
])

class Command:
    HELLO = 0
    ADD_NODE = 1
    REMOVE_NODE = 9
    RENAME_NODE = 23
    SCAN_DANGLING = 24
    LOOKUP_NODE_ID = 12
    ADD_PROPERTY = 11
    LINK_NODE = 2
    UNLINK_NODE = 8
    GET_INFO = 19
    GET_CHILDREN = 4
    GET_PARENTS = 5
    GET_PROPERTIES = 3
    GET_PROPERTY_VALUE = 6
    SET_PROPERTY_VALUE = 7
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
    WINDOW_INPUT = 3
    KEYBOARD = 4
    MOUSE = 5
    LAYER = 6
    OBJECT = 7
    VECTOR_ART = 8
    TEXT = 9
    TEXTURE = 10
    FONTS = 11
    FONT = 12
    CHAT_VIEW = 13
    EDIT = 14
    IMAGE = 15
    BUTTON = 16
    SHORTCUT = 17
    GESTURE = 18
    EMOJI_PICKER = 19
    SETTING = 21
    MENU = 22
    TOKEN_TABLE = 23
    TEXT_SCRAMBLE = 24
    PLUGIN_ROOT = 100
    PLUGIN = 101

class PropertyType:
    NULL = 0
    BOOL = 1
    UINT32 = 2
    FLOAT32 = 3
    STR = 4
    ENUM = 5
    SCENE_NODE_ID = 7
    SEXPR = 8
    VECTOR_SHAPE = 9

    @staticmethod
    def to_str(prop_type):
        match prop_type:
            case PropertyType.NULL:
                return "null"
            case PropertyType.BOOL:
                return "bool"
            case PropertyType.UINT32:
                return "uint32"
            case PropertyType.FLOAT32:
                return "float32"
            case PropertyType.STR:
                return "str"
            case PropertyType.ENUM:
                return "enum"
            case PropertyType.SCENE_NODE_ID:
                return "scene_node_id"
            case PropertyType.SEXPR:
                return "sexpr"
            case PropertyType.VECTOR_SHAPE:
                return "vector_shape"

class PropertySubType:
    NULL = 0
    COLOR = 1
    PIXEL = 2
    RESOURCE_ID = 3
    LOCALE = 4
    FLAG = 5

    @staticmethod
    def to_str(prop_type):
        match prop_type:
            case PropertySubType.NULL:
                return "null"
            case PropertySubType.COLOR:
                return "color"
            case PropertySubType.PIXEL:
                return "pixel"
            case PropertySubType.RESOURCE_ID:
                return "resource_id"
            case PropertySubType.LOCALE:
                return "locale"
            case PropertySubType.FLAG:
                return "flag"

class CallArgType:
    UINT32 = 0
    UINT64 = 1
    FLOAT32 = 2
    BOOL = 3
    STR = 4
    HASH = 5

    @staticmethod
    def to_str(arg_type):
        match arg_type:
            case CallArgType.UINT32:
                return "uint32"
            case CallArgType.UINT64:
                return "uint64"
            case CallArgType.FLOAT32:
                return "float32"
            case CallArgType.BOOL:
                return "bool"
            case CallArgType.STR:
                return "str"
            case CallArgType.HASH:
                return "hash"
            case _:
                return "unknown"

class PropertyStatus:
    OK = 0
    UNSET = 1
    NULL = 2
    EXPR = 3

class ErrorCode:
    INVALID_SCENE_PATH = 1
    NODE_NOT_FOUND = 2
    CHILD_NODE_NOT_FOUND = 3
    PARENT_NODE_NOT_FOUND = 4
    PROPERTY_ALREADY_EXISTS = 5
    PROPERTY_NOT_FOUND = 6
    PROPERTY_WRONG_TYPE = 7
    PROPERTY_WRONG_LEN = 9
    PROPERTY_WRONG_INDEX = 10
    PROPERTY_OUT_OF_RANGE = 11
    PROPERTY_NULL_NOT_ALLOWED = 12
    PROPERTY_SEXPR_NOT_ALLOWED = 13
    PROPERTY_IS_BOUNDED = 14
    PROPERTY_WRONG_ENUM_ITEM = 15
    SIGNAL_ALREADY_EXISTS = 16
    SIGNAL_NOT_FOUND = 17
    SLOT_NOT_FOUND = 18
    METHOD_ALREADY_EXISTS = 19
    METHOD_NOT_FOUND = 20
    NODES_ARE_LINKED = 21
    NODES_NOT_LINKED = 22
    NODE_HAS_PARENTS = 23
    NODE_HAS_CHILDREN = 24
    NODE_PARENT_NAME_CONFLICT = 25
    NODE_CHILD_NAME_CONFLICT = 26
    NODE_SIBLING_NAME_CONFLICT = 27
    SEXPR_GLOBAL_NOT_FOUND = 32
    PUBLISHER_DESTROYED = 34
    CHANNEL_CLOSED = 36
    NODES_ARE_SAME = 37
    UNEXPECTED_TOKEN = 38
    KVDB_ERR = 39
    SERVICE_FAILED = 40
    GFX_DUPLICATE_TEXTURE_ID = 41
    GFX_UNKNOWN_TEXTURE_ID = 42
    GFX_DUPLICATE_BUFFER_ID = 43
    GFX_UNKNOWN_BUFFER_ID = 44
    GFX_DUPLICATE_ANIM_ID = 45
    GFX_UNKNOWN_ANIM_ID = 46
    CONTACT_NOT_FOUND = 47
    SERIAL_ERR = 48
    TURSO_ERR = 49
    UNSUPPORTED_NODE_TYPE = 50
    NODE_NOT_REMOVABLE = 51

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
            case ErrorCode.PROPERTY_WRONG_LEN:
                return "property_wrong_len"
            case ErrorCode.PROPERTY_WRONG_INDEX:
                return "property_wrong_index"
            case ErrorCode.PROPERTY_OUT_OF_RANGE:
                return "property_out_of_range"
            case ErrorCode.PROPERTY_NULL_NOT_ALLOWED:
                return "property_null_not_allowed"
            case ErrorCode.PROPERTY_SEXPR_NOT_ALLOWED:
                return "property_sexpr_not_allowed"
            case ErrorCode.PROPERTY_IS_BOUNDED:
                return "property_is_bounded"
            case ErrorCode.PROPERTY_WRONG_ENUM_ITEM:
                return "property_wrong_enum_item"
            case ErrorCode.SIGNAL_ALREADY_EXISTS:
                return "signal_already_exists"
            case ErrorCode.SIGNAL_NOT_FOUND:
                return "signal_not_found"
            case ErrorCode.SLOT_NOT_FOUND:
                return "slot_not_found"
            case ErrorCode.METHOD_ALREADY_EXISTS:
                return "method_already_exists"
            case ErrorCode.METHOD_NOT_FOUND:
                return "method_not_found"
            case ErrorCode.NODES_ARE_LINKED:
                return "nodes_are_linked"
            case ErrorCode.NODES_NOT_LINKED:
                return "nodes_not_linked"
            case ErrorCode.NODE_HAS_PARENTS:
                return "node_has_parents"
            case ErrorCode.NODE_HAS_CHILDREN:
                return "node_has_children"
            case ErrorCode.NODE_PARENT_NAME_CONFLICT:
                return "node_parent_name_conflict"
            case ErrorCode.NODE_CHILD_NAME_CONFLICT:
                return "node_child_name_conflict"
            case ErrorCode.NODE_SIBLING_NAME_CONFLICT:
                return "node_sibling_name_conflict"
            case ErrorCode.SEXPR_GLOBAL_NOT_FOUND:
                return "sexpr_global_not_found"
            case ErrorCode.PUBLISHER_DESTROYED:
                return "publisher_destroyed"
            case ErrorCode.CHANNEL_CLOSED:
                return "channel_closed"
            case ErrorCode.NODES_ARE_SAME:
                return "nodes_are_same"
            case ErrorCode.UNEXPECTED_TOKEN:
                return "unexpected_token"
            case ErrorCode.KVDB_ERR:
                return "kvdb_err"
            case ErrorCode.SERVICE_FAILED:
                return "service_failed"
            case ErrorCode.GFX_DUPLICATE_TEXTURE_ID:
                return "gfx_duplicate_texture_id"
            case ErrorCode.GFX_UNKNOWN_TEXTURE_ID:
                return "gfx_unknown_texture_id"
            case ErrorCode.GFX_DUPLICATE_BUFFER_ID:
                return "gfx_duplicate_buffer_id"
            case ErrorCode.GFX_UNKNOWN_BUFFER_ID:
                return "gfx_unknown_buffer_id"
            case ErrorCode.GFX_DUPLICATE_ANIM_ID:
                return "gfx_duplicate_anim_id"
            case ErrorCode.GFX_UNKNOWN_ANIM_ID:
                return "gfx_unknown_anim_id"
            case ErrorCode.CONTACT_NOT_FOUND:
                return "contact_not_found"
            case ErrorCode.SERIAL_ERR:
                return "serial_err"
            case ErrorCode.TURSO_ERR:
                return "turso_err"
            case ErrorCode.UNSUPPORTED_NODE_TYPE:
                return "unsupported_node_type"
            case ErrorCode.NODE_NOT_REMOVABLE:
                return "node_not_removable"
            case _:
                return "unknown"

class Api:

    def __init__(self, addr="127.0.0.1", port=9484):
        self.addr = addr
        self.port = port
        self.context = zmq.Context()
        self.socket = self._make_socket()

    def _make_socket(self):
        socket = self.context.socket(zmq.REQ)
        #self.socket.setsockopt(zmq.IPV6, True)
        # Fail fast with zmq.error.Again when no app is listening, so the
        # CLI can report the endpoint it tried instead of hanging forever.
        socket.setsockopt(zmq.RCVTIMEO, 3000)
        # Discard undelivered messages at exit instead of blocking on
        # context teardown when no app ever answered.
        socket.setsockopt(zmq.LINGER, 0)
        socket.connect(f"tcp://{self.addr}:{self.port}")
        return socket

    def _reset_socket(self):
        # A REQ socket whose reply timed out is stuck mid-request and
        # rejects further sends; replace it so later requests work.
        try:
            self.socket.close(linger=0)
        except zmq.error.ZMQError:
            pass
        self.socket = self._make_socket()

    def _make_request(self, cmd, payload):
        req_cmd = bytearray()
        serial.write_u8(req_cmd, cmd)
        try:
            self.socket.send_multipart([req_cmd, payload])
            errc, reply = self.socket.recv_multipart()
        except zmq.error.ZMQError:
            self._reset_socket()
            raise
        errc = int.from_bytes(errc, "little")
        cursor = serial.Cursor(reply)
        match errc:
            case 0:
                pass
            case ErrorCode.INVALID_SCENE_PATH:
                raise exc.InvalidScenePath
            case ErrorCode.NODE_NOT_FOUND:
                raise exc.NodeNotFound
            case ErrorCode.CHILD_NODE_NOT_FOUND:
                raise exc.ChildNodeNotFound
            case ErrorCode.PARENT_NODE_NOT_FOUND:
                raise exc.ParentNodeNotFound
            case ErrorCode.PROPERTY_ALREADY_EXISTS:
                raise exc.PropertyAlreadyExists
            case ErrorCode.PROPERTY_NOT_FOUND:
                raise exc.PropertyNotFound
            case ErrorCode.PROPERTY_WRONG_TYPE:
                raise exc.PropertyWrongType
            case ErrorCode.PROPERTY_WRONG_LEN:
                raise exc.PropertyWrongLen
            case ErrorCode.PROPERTY_WRONG_INDEX:
                raise exc.PropertyWrongIndex
            case ErrorCode.PROPERTY_OUT_OF_RANGE:
                raise exc.PropertyOutOfRange
            case ErrorCode.PROPERTY_NULL_NOT_ALLOWED:
                raise exc.PropertyNullNotAllowed
            case ErrorCode.PROPERTY_SEXPR_NOT_ALLOWED:
                raise exc.PropertySExprNotAllowed
            case ErrorCode.PROPERTY_IS_BOUNDED:
                raise exc.PropertyIsBounded
            case ErrorCode.PROPERTY_WRONG_ENUM_ITEM:
                raise exc.PropertyWrongEnumItem
            case ErrorCode.SIGNAL_ALREADY_EXISTS:
                raise exc.SignalAlreadyExists
            case ErrorCode.SIGNAL_NOT_FOUND:
                raise exc.SignalNotFound
            case ErrorCode.SLOT_NOT_FOUND:
                raise exc.SlotNotFound
            case ErrorCode.METHOD_ALREADY_EXISTS:
                raise exc.MethodAlreadyExists
            case ErrorCode.METHOD_NOT_FOUND:
                raise exc.MethodNotFound
            case ErrorCode.NODES_ARE_LINKED:
                raise exc.NodesAreLinked
            case ErrorCode.NODES_NOT_LINKED:
                raise exc.NodesNotLinked
            case ErrorCode.NODE_HAS_PARENTS:
                raise exc.NodeHasParents
            case ErrorCode.NODE_HAS_CHILDREN:
                raise exc.NodeHasChildren
            case ErrorCode.NODE_PARENT_NAME_CONFLICT:
                raise exc.NodeParentNameConflict
            case ErrorCode.NODE_CHILD_NAME_CONFLICT:
                raise exc.NodeChildNameConflict
            case ErrorCode.NODE_SIBLING_NAME_CONFLICT:
                raise exc.NodeSiblingNameConflict
            case ErrorCode.SEXPR_GLOBAL_NOT_FOUND:
                raise exc.SExprGlobalNotFound
            case ErrorCode.PUBLISHER_DESTROYED:
                raise exc.PublisherDestroyed
            case ErrorCode.CHANNEL_CLOSED:
                raise exc.ChannelClosed
            case ErrorCode.NODES_ARE_SAME:
                raise exc.NodesAreSame
            case ErrorCode.UNEXPECTED_TOKEN:
                raise exc.UnexpectedToken
            case ErrorCode.KVDB_ERR:
                raise exc.KvdbErr
            case ErrorCode.SERVICE_FAILED:
                raise exc.ServiceFailed
            case ErrorCode.GFX_DUPLICATE_TEXTURE_ID:
                raise exc.GfxDuplicateTextureID
            case ErrorCode.GFX_UNKNOWN_TEXTURE_ID:
                raise exc.GfxUnknownTextureID
            case ErrorCode.GFX_DUPLICATE_BUFFER_ID:
                raise exc.GfxDuplicateBufferID
            case ErrorCode.GFX_UNKNOWN_BUFFER_ID:
                raise exc.GfxUnknownBufferID
            case ErrorCode.GFX_DUPLICATE_ANIM_ID:
                raise exc.GfxDuplicateAnimID
            case ErrorCode.GFX_UNKNOWN_ANIM_ID:
                raise exc.GfxUnknownAnimID
            case ErrorCode.CONTACT_NOT_FOUND:
                raise exc.ContactNotFound
            case ErrorCode.SERIAL_ERR:
                raise exc.SerialErr
            case ErrorCode.TURSO_ERR:
                raise exc.TursoErr
            case ErrorCode.UNSUPPORTED_NODE_TYPE:
                raise exc.UnsupportedNodeType
            case ErrorCode.NODE_NOT_REMOVABLE:
                raise exc.NodeNotRemovable
            case _:
                raise exc.UnknownError(f"unknown error code: {errc}")
        return cursor

    def hello(self):
        response = self._make_request(Command.HELLO, bytearray())
        return serial.decode_str(response)

    def get_children(self, node_path):
        req = bytearray()
        serial.encode_str(req, node_path)
        cur = self._make_request(Command.GET_CHILDREN, req)
        children_len = serial.decode_varint(cur)
        children = []
        for _ in range(children_len):
            child_name = serial.decode_str(cur)
            child_id = serial.read_u32(cur)
            child_type = serial.read_u8(cur)
            children.append((child_name, child_id, child_type))
        return children

    def get_properties(self, node_path):
        req = bytearray()
        serial.encode_str(req, node_path)
        cur = self._make_request(Command.GET_PROPERTIES, req)
        props_len = serial.decode_varint(cur)
        props = []

        enum_read_fn = lambda cur: serial.decode_arr(cur, serial.decode_str)

        def depend_read_fn(cur):
            i = serial.read_u32(cur)
            local_name = serial.decode_str(cur)
            return (i, local_name)

        for _ in range(props_len):
            prop_name = serial.decode_str(cur)
            # We need prop_type below
            prop_type = serial.read_u8(cur)

            prop_read_fn = lambda cur: Api.read_prop_val(cur, prop_type)

            prop = Property(
                prop_name,
                prop_type,
                # subtype 
                serial.read_u8(cur),
                # defaults 
                #serial.decode_arr(cur, prop_read_fn),
                # ui_name 
                serial.decode_str(cur),
                # desc 
                serial.decode_str(cur),
                # is_null_allowed 
                bool(serial.read_u8(cur)),
                # is_expr_allowed 
                bool(serial.read_u8(cur)),
                # array_len 
                serial.read_u32(cur),
                # min_val 
                serial.decode_opt(cur, prop_read_fn),
                # max_val 
                serial.decode_opt(cur, prop_read_fn),
                # enum_items 
                serial.decode_opt(cur, enum_read_fn),
                # depends
                serial.decode_arr(cur, depend_read_fn)
            )
            props.append(prop)
        return props

    @staticmethod
    def read_prop_val(cur, prop_type):
        match prop_type:
            case PropertyType.NULL:
                return None
            case PropertyType.BOOL:
                return bool(serial.read_u8(cur))
            case PropertyType.UINT32:
                return serial.read_u32(cur)
            case PropertyType.FLOAT32:
                return serial.read_f32(cur)
            case PropertyType.STR:
                return serial.decode_str(cur)
            case PropertyType.ENUM:
                return serial.decode_str(cur)
            case PropertyType.SCENE_NODE_ID:
                return serial.read_u32(cur)
            case PropertyType.VECTOR_SHAPE:
                # Shapes carry no payload on the get path
                return "<...>"
            case _:
                raise Exception("unknown property type returned")

    def get_property_value_full(self, node_path, prop_name):
        req = bytearray()
        serial.encode_str(req, node_path)
        serial.encode_str(req, prop_name)
        cur = self._make_request(Command.GET_PROPERTY_VALUE, req)
        prop_type = serial.read_u8(cur)

        def prop_read_fn(cur):
            prop_status = serial.read_u8(cur)
            match prop_status:
                case PropertyStatus.NULL:
                    return (PropertyStatus.NULL, None)
                case PropertyStatus.EXPR:
                    return (PropertyStatus.EXPR, Expr(serial.decode_str(cur)))
                case PropertyStatus.UNSET | PropertyStatus.OK:
                    return (prop_status, Api.read_prop_val(cur, prop_type))

        return serial.decode_arr(cur, prop_read_fn)

    def get_property_value(self, node_path, prop_name):
        vals = self.get_property_value_full(node_path, prop_name)
        return [val for (_, val) in vals]

    def add_node(self, parent_path, name, node_type):
        req = bytearray()
        serial.encode_str(req, parent_path)
        serial.encode_str(req, name)
        serial.write_u8(req, int(node_type))
        cur = self._make_request(Command.ADD_NODE, req)
        node_id = serial.read_u32(cur)
        return node_id

    def remove_node(self, node_path):
        req = bytearray()
        serial.encode_str(req, node_path)
        self._make_request(Command.REMOVE_NODE, req)

    def set_property_null(self, node_path, prop_name, i):
        req = bytearray()
        serial.encode_str(req, node_path)
        serial.encode_str(req, prop_name)
        serial.write_u32(req, i)
        serial.write_u8(req, PropertyType.NULL)
        self._make_request(Command.SET_PROPERTY_VALUE, req)

    def set_property_bool(self, node_path, prop_name, i, val):
        req = bytearray()
        serial.encode_str(req, node_path)
        serial.encode_str(req, prop_name)
        serial.write_u32(req, i)
        serial.write_u8(req, PropertyType.BOOL)
        serial.write_u8(req, int(val))
        self._make_request(Command.SET_PROPERTY_VALUE, req)

    def set_property_u32(self, node_path, prop_name, i, val):
        req = bytearray()
        serial.encode_str(req, node_path)
        serial.encode_str(req, prop_name)
        serial.write_u32(req, i)
        serial.write_u8(req, PropertyType.UINT32)
        serial.write_u32(req, val)
        self._make_request(Command.SET_PROPERTY_VALUE, req)

    def set_property_f32(self, node_path, prop_name, i, val):
        req = bytearray()
        serial.encode_str(req, node_path)
        serial.encode_str(req, prop_name)
        serial.write_u32(req, i)
        serial.write_u8(req, PropertyType.FLOAT32)
        serial.write_f32(req, val)
        self._make_request(Command.SET_PROPERTY_VALUE, req)

    def set_property_str(self, node_path, prop_name, i, val):
        req = bytearray()
        serial.encode_str(req, node_path)
        serial.encode_str(req, prop_name)
        serial.write_u32(req, i)
        serial.write_u8(req, PropertyType.STR)
        serial.encode_str(req, val)
        self._make_request(Command.SET_PROPERTY_VALUE, req)

    def set_property_enum(self, node_path, prop_name, i, val):
        req = bytearray()
        serial.encode_str(req, node_path)
        serial.encode_str(req, prop_name)
        serial.write_u32(req, i)
        serial.write_u8(req, PropertyType.ENUM)
        serial.encode_str(req, val)
        self._make_request(Command.SET_PROPERTY_VALUE, req)

    def set_property_node_id(self, node_path, prop_name, i, val):
        req = bytearray()
        serial.encode_str(req, node_path)
        serial.encode_str(req, prop_name)
        serial.write_u32(req, i)
        serial.write_u8(req, PropertyType.SCENE_NODE_ID)
        serial.write_u32(req, val)
        self._make_request(Command.SET_PROPERTY_VALUE, req)

    def set_property_expr(self, node_path, prop_name, i, expr_str):
        req = bytearray()
        serial.encode_str(req, node_path)
        serial.encode_str(req, prop_name)
        serial.write_u32(req, i)
        serial.write_u8(req, PropertyType.SEXPR)
        serial.encode_str(req, expr_str)
        self._make_request(Command.SET_PROPERTY_VALUE, req)

    def set_property_shape(self, node_path, prop_name, i, verts, indices):
        # verts: list of (x_expr, y_expr, [r, g, b, a]) tuples, where the
        # coordinate exprs use the same source language as set_property_expr
        req = bytearray()
        serial.encode_str(req, node_path)
        serial.encode_str(req, prop_name)
        serial.write_u32(req, i)
        serial.write_u8(req, PropertyType.VECTOR_SHAPE)
        serial.encode_varint(req, len(verts))
        for (x_expr, y_expr, color) in verts:
            serial.encode_str(req, x_expr)
            serial.encode_str(req, y_expr)
            for c in color:
                serial.write_f32(req, c)
        serial.encode_varint(req, len(indices))
        for index in indices:
            serial.write_u16(req, index)
        self._make_request(Command.SET_PROPERTY_VALUE, req)

    def get_signals(self, node_path):
        req = bytearray()
        serial.encode_str(req, node_path)
        cur = self._make_request(Command.GET_SIGNALS, req)
        sigs_len = serial.decode_varint(cur)
        sigs = []
        for _ in range(sigs_len):
            sigs.append(serial.decode_str(cur))
        return sigs

    def register_slot(self, node_path, sig_name, slot_name, user_data):
        req = bytearray()
        serial.encode_str(req, node_path)
        serial.encode_str(req, sig_name)
        serial.encode_str(req, slot_name)
        serial.encode_varint(req, len(user_data))
        req += user_data
        cur = self._make_request(Command.REGISTER_SLOT, req)
        slot_id = serial.read_u32(cur)
        return slot_id

    def get_slots(self, node_path, sig_name):
        req = bytearray()
        serial.encode_str(req, node_path)
        serial.encode_str(req, sig_name)
        cur = self._make_request(Command.GET_SLOTS, req)

        def read_slot(cur):
            slot_name = serial.decode_str(cur)
            slot_id = serial.read_u32(cur)
            return (slot_name, slot_id)

        slots = serial.decode_arr(cur, read_slot)
        return slots

    def get_methods(self, node_path):
        req = bytearray()
        serial.encode_str(req, node_path)
        cur = self._make_request(Command.GET_METHODS, req)

        def read_method(cur):
            method_name = serial.decode_str(cur)
            return method_name

        methods = serial.decode_arr(cur, read_method)
        return methods

    def get_method(self, node_path, method_name):
        req = bytearray()
        serial.encode_str(req, node_path)
        serial.encode_str(req, method_name)
        cur = self._make_request(Command.GET_METHOD, req)

        def read_arg(cur):
            arg_name = serial.decode_str(cur)
            arg_desc = serial.decode_str(cur)
            arg_type = serial.read_u8(cur)
            return (arg_name, arg_desc, arg_type)

        args = serial.decode_arr(cur, read_arg)
        results = serial.decode_opt(cur, lambda cur: serial.decode_arr(cur, read_arg))

        return (args, results)

    def call_method(self, node_path, method_name, arg_data):
        req = bytearray()
        serial.encode_str(req, node_path)
        serial.encode_str(req, method_name)
        serial.encode_buf(req, arg_data)
        cur = self._make_request(Command.CALL_METHOD, req)
        result = serial.decode_opt(cur, serial.decode_buf)
        return result

