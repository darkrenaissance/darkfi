import requests
import json


def main():
    url = "http://localhost:8000/"

    # Example echo method
    payload = {
        "method": "stop",
        #"method": "get_info",
        "params": [],
        "jsonrpc": "2.0",
        "id": 0,
    }
    response = requests.post(url, json=payload).json()

    print(response)
    #assert response["result"] == "Hello World!"
    assert response["jsonrpc"]

if __name__ == "__main__":
    main()

