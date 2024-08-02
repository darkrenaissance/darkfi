from gui import *

node_id = api.lookup_node_id("/window/view/chatty")

arg_data = bytearray()
serial.write_u32(arg_data, 110)
serial.encode_str(arg_data, "nick")
serial.encode_str(arg_data, "hello1234")

api.call_method(node_id, "insert_line", arg_data)

