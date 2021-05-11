import requests

# TODO: generate random ID (4 byte unsigned int) (rand range 0 - max size uint32)
# TODO: make functions async
# TODO: parse json replies into something more legible

class RpcClient:
    def __init__(self):
        self.url = "http://localhost:8000/"
        self.payload = {
            "method": [],
            "params": [],
            "jsonrpc": [],
            "id": [],
        }

    def key_gen(self, payload):
        payload['method'] = "key_gen"
        payload['jsonrpc'] = "2.0"
        payload['id'] = "0"
        key = self.__request(payload)
        print(key)

    def get_info(self, payload):
        payload['method'] = "get_info"
        payload['jsonrpc'] = "2.0"
        payload['id'] = "0"
        info = self.__request(payload)
        print(info)

    def stop(self, payload):
        payload['method'] = "stop"
        payload['jsonrpc'] = "2.0"
        payload['id'] = "0"
        stop = self.__request(payload)
        print(stop)

    def say_hello(self, payload):
        payload['method'] = "say_hello"
        payload['jsonrpc'] = "2.0"
        payload['id'] = "0"
        hello = self.__request(payload)
        print(hello)

    def __request(self, payload):
        response = requests.post(self.url, json=payload).json()
        print(response)
        assert response["jsonrpc"]

    
if __name__ == "__main__":
    client = RpcClient()

