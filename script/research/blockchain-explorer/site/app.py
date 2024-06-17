from flask import Flask, render_template

import rpc

# DarkFi blockchain-explorer daemon JSON-RPC configuration
# TODO: make this configurable
URL = "127.0.0.1"
PORT = 14567

app = Flask(__name__)

@app.route('/')
async def index():
    blocks = await rpc.get_last_n_blocks(10, URL, PORT)
    stats = await rpc.get_basic_statistics(URL, PORT)
    return render_template('index.html', blocks=blocks, stats=stats)

