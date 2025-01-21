from . import serial, ErrorCode

class HostApi:

    def __init__(self, api):
        self.api = api

    def create_text(self, font_path, node_name, text, font_size):
        call_data = bytearray()
        serial.encode_str(call_data, node_name)
        serial.encode_str(call_data, text)
        serial.write_f32(call_data, font_size)
        font_id = self.api.lookup_node_id(font_path)
        (errc, result) = self.api.call_method(font_id, "create_text", call_data)
        if errc != 0:
            print("create_text error:", ErrorCode.to_str(errc))
            return None
        text_id = int.from_bytes(result, "little")
        return text_id

    def load_texture(self, node_name, filepath):
        call_data = bytearray()
        serial.encode_str(call_data, node_name)
        serial.encode_str(call_data, filepath)
        win_id = self.api.lookup_node_id("/window")
        (errc, result) = self.api.call_method(win_id, "load_texture", call_data)
        if errc != 0:
            print("load_texture error:", ErrorCode.to_str(errc))
            return None
        texture_id = int.from_bytes(result, "little")
        return texture_id


