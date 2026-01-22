#!/usr/bin/env python3
from datetime import datetime, timezone

from quart import Quart, render_template, abort, g

from rpc_client import JsonRpcPool, JsonRpcError

app = Quart(__name__)
app.config.update(
    RPC_HOST="127.0.0.1",
    RPC_PORT="22222",
    RPC_MIN_CONNECTIONS=5,
    RPC_MAX_CONNECTIONS=50,
    NETWORK="testnet",
)

# Global pool
rpc: JsonRpcPool = None


@app.before_serving
async def startup():
    global rpc
    rpc = JsonRpcPool(
        host=app.config["RPC_HOST"],
        port=app.config["RPC_PORT"],
        min_connections=app.config["RPC_MIN_CONNECTIONS"],
        max_connections=app.config["RPC_MAX_CONNECTIONS"],
    )
    await rpc.start()
    app.logger.info(f"RPC pool started: {app.config['RPC_HOST']}:{app.config['RPC_PORT']}")


@app.after_serving
async def shutdown():
    await rpc.close()
    app.logger.info("RPC pool closed")


@app.errorhandler(JsonRpcError)
async def handle_rpc_error(error: JsonRpcError):
    app.logger.error(f"RPC Error: {error.code} - {error.message}")
    if error.code == -32600:
        abort(404)
    return await render_template("error.html", error=error.message), 500


@app.errorhandler(ConnectionError)
async def handle_connection_error(error):
    app.logger.error(f"RPC Connection Error: {error}")
    return await render_template("error.html", error="Service temporarily unavailable"), 503


@app.route("/")
async def index():
    current_difficulty = await rpc.call("current_difficulty", params=[])
    current_height = await rpc.call("current_height", params=[])
    latest_blocks = await rpc.call("latest_blocks", params=[20])

    # TODO: hashrate
    # TODO: emission
    # TODO: mempool_txs = await rpc.call("mempool", params=[])
    
    for block in latest_blocks:
        dt = datetime.fromtimestamp(block["timestamp"], tz=timezone.utc)
        block["timestamp"] = dt.strftime("%B %d, %Y at %I:%M %p UTC")

    return await render_template(
        "index.html",
        network=app.config["NETWORK"],
        current_difficulty=current_difficulty[0],
        current_height=current_height,
        #mempool_txs=mempool_txs,
        #mempool_txs_len=len(mempool_txs),
        latest_blocks=latest_blocks,
    )


@app.route("/block/<int:block_height>")
async def get_block_by_height(block_height: int):
    current_difficulty = await rpc.call("current_difficulty", params=[])
    current_height = await rpc.call("current_height", params=[])
    block = await rpc.call("get_block", params=[block_height])    

    dt = datetime.fromtimestamp(block["timestamp"], tz=timezone.utc)
    block["timestamp"] = dt.strftime("%B %d, %Y at %I:%M %p UTC")
    block["n_txs"] = len(block["txs"])

    return await render_template(
        "block.html",
        network=app.config["NETWORK"],
        current_difficulty=current_difficulty[0],
        current_height=current_height,
        block=block,
    )


@app.route("/tx/<tx_hash>")
async def get_tx_by_hash(tx_hash: str):
    current_difficulty = await rpc.call("current_difficulty", params=[])
    current_height = await rpc.call("current_height", params=[])
    tx = await rpc.call("get_tx", params=[tx_hash])

    return await render_template(
        "tx.html",
        network=app.config["NETWORK"],
        current_difficulty=current_difficulty[0],
        current_height=current_height,
        tx=tx,
    )


if __name__ == "__main__":
    app.run(host="127.0.0.1", port=5000, debug=True)
