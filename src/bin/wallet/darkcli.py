import argparse
import requests
import json

def arg_parser(DarkClient):
    parser = argparse.ArgumentParser(prog='dark',
                                          usage='%(prog)s [commands]',
                                          description="""DarkFi wallet
                                          command-line tool""")
    parser.add_argument("new", help="Calls a new method")
    parser.add_argument("-k", "--key", action='store_true', help="Generate a new keypair")
    parser.add_argument("-i", "--info", action='store_true', help="Request info from daemon")
    args = parser.parse_args()

    if args.key:
        try:
            print("Attemping to generate a new key pair...")
            client.key_gen()
        except Exception:
            print("Something went wrong")
            raise

    if args.info:
        try:
            print("Info was entered")
            client.get_info()
            print("Requesting daemon info...")
        except Exception:
            print("Something went wrong")
            raise


class DarkClient:
    # generate random ID (4 byte unsigned int) (rand range 0 - max size
    # uint32
    def __init__(self, rpc_id = None, params = None, method = None):
        print("init called")
        self.url = "http://localhost:8000/"
        self.rpc_id = rpc_id
        self.params = params
        self.method = method
        print("init done")

    def test_method(self, rpc_id, method):
        print("Test variables are:\n" + "url:\n" + str(self.url) + "" + str(rpc_id)  + " " + str(method))

    def key_gen(self):
        rpc_id = 0 
        method = "key_gen"
        params = None
        self.__request(rpc_id, params, method)

    def get_info(self):
        rpc_id = 1
        method = "get_info",
        params = []
        self.__request(rpc_id, params, method)

    def __request(self, rpc_id, params, method):
        payload = {
            "method": method,
            "params": params,
            "jsonrpc": "2.0",
            "id": rpc_id,
        }
        payload['method'] = 'get_info'
        payload['id'] = 0
        response = requests.post(self.url, json=payload).json()
        # print something better
        print(response)
        assert response["jsonrpc"]

    
if __name__ == "__main__":
    client = DarkClient()
    arg_parser(client)

    #rpc()
    ## Example echo method
    #payload = {
    #    #"method:": args,
    #    #"method": "stop",
    #    "method": "get_info",
    #    #"method": "say_hello",
    #    #"params": [],
    #    "jsonrpc": "2.0",
    #    "id": 0,
    #}
    #response = requests.post(url, json=payload).json()

    #print(response)
    #assert response["result"] == "Hello World!"
    #assert response["jsonrpc"]
