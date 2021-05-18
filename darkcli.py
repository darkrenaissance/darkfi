# TODO: refactor into async

import argparse
import requests
import json

def arg_parser(client):
    parser = argparse.ArgumentParser(prog='dark',
                                          usage='%(prog)s [commands]',
                                          description="""DarkFi wallet
                                          command-line tool""")
    parser.add_argument("-k", "--key", action='store_true', help="Generate a new keypair")
    parser.add_argument("-i", "--info", action='store_true', help="Request info from daemon")
    parser.add_argument("-s", "--stop", action='store_true', help="Send a stop signal to the daemon")
    parser.add_argument("-hi", "--hello", action='store_true', help="Say hello")
    args = parser.parse_args()

    if args.key:
        try:
            print("Attemping to generate a new key pair...")
            client.key_gen(client.payload)
        except Exception:
            raise

    if args.info:
        try:
            print("Info was entered")
            client.get_info(client.payload)
            print("Requesting daemon info...")
        except Exception:
            raise

    if args.stop:
        try:
            print("Stop was entered")
            client.stop(client.payload)
            print("Sending a stop signal...")
        except Exception:
            raise

    if args.hello:
        try:
            print("Hello was entered")
            client.say_hello(client.payload)
        except Exception:
            raise


class DarkClient:
    # TODO: generate random ID (4 byte unsigned int) (rand range 0 - max size
    # uint32
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
        # print something better
        # parse into data structure 
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
