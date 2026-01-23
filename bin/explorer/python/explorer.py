#!/usr/bin/env python3
# This file is part of DarkFi (https://dark.fi)
#
# Copyright (C) 2020-2026 Dyne.org foundation
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU Affero General Public License as
# published by the Free Software Foundation, either version 3 of the
# License, or (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU Affero General Public License for more details.
#
# You should have received a copy of the GNU Affero General Public License
# along with this program.  If not, see <https://www.gnu.org/licenses/>.
from datetime import datetime, timezone

from quart import Quart, render_template, abort, request, redirect, url_for

from rpc_client import JsonRpcPool, JsonRpcError

app = Quart(__name__)
app.config.update(
    RPC_HOST="127.0.0.1",
    RPC_PORT="22222",
    RPC_MIN_CONNECTIONS=5,
    RPC_MAX_CONNECTIONS=50,
    NETWORK="Testnet",
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
        return await render_template(
            "error.html",
            network=app.config["NETWORK"],
            error_code="404",
            error="The requested resource was not found"
        ), 404
    return await render_template(
        "error.html",
        network=app.config["NETWORK"],
        error_code="500",
        error=error.message
    ), 500


@app.errorhandler(ConnectionError)
async def handle_connection_error(error):
    app.logger.error(f"RPC Connection Error: {error}")
    return await render_template(
        "error.html",
        network=app.config["NETWORK"],
        error_code="503",
        error="Service temporarily unavailable"
    ), 503


@app.errorhandler(404)
async def handle_not_found(error):
    return await render_template(
        "error.html",
        network=app.config["NETWORK"],
        error_code="404",
        error="Page not found"
    ), 404


def format_hashrate(hashrate: float) -> str:
    """Format hashrate with appropriate unit."""
    if hashrate >= 1e12:
        return f"{hashrate / 1e12:.2f} TH/s"
    elif hashrate >= 1e9:
        return f"{hashrate / 1e9:.2f} GH/s"
    elif hashrate >= 1e6:
        return f"{hashrate / 1e6:.2f} MH/s"
    elif hashrate >= 1e3:
        return f"{hashrate / 1e3:.2f} KH/s"
    else:
        return f"{hashrate:.2f} H/s"


@app.route("/")
async def index():
    current_difficulty = await rpc.call("current_difficulty", params=[])
    current_height = await rpc.call("current_height", params=[])
    latest_blocks = await rpc.call("latest_blocks", params=[20])
    hashrate = await rpc.call("get_hashrate", params=[])

    for block in latest_blocks:
        dt = datetime.fromtimestamp(block["timestamp"], tz=timezone.utc)
        block["timestamp"] = dt.strftime("%B %d, %Y at %I:%M %p UTC")

    return await render_template(
        "index.html",
        network=app.config["NETWORK"],
        current_difficulty=current_difficulty[0],
        current_height=current_height,
        hashrate=format_hashrate(hashrate),
        latest_blocks=latest_blocks,
    )


@app.route("/block/<int:block_height>")
async def get_block_by_height(block_height: int):
    if block_height < 0:
        abort(404)

    current_difficulty = await rpc.call("current_difficulty", params=[])
    current_height = await rpc.call("current_height", params=[])
    hashrate = await rpc.call("get_hashrate", params=[])
    block = await rpc.call("get_block", params=[block_height])

    dt = datetime.fromtimestamp(block["timestamp"], tz=timezone.utc)
    block["timestamp"] = dt.strftime("%B %d, %Y at %I:%M %p UTC")
    block["n_txs"] = len(block["txs"])

    return await render_template(
        "block.html",
        network=app.config["NETWORK"],
        current_difficulty=current_difficulty[0],
        current_height=current_height,
        hashrate=format_hashrate(hashrate),
        block=block,
    )


@app.route("/tx/<tx_hash>")
async def get_tx_by_hash(tx_hash: str):
    # Validate hex string
    if not all(c in '0123456789abcdefABCDEF' for c in tx_hash):
        abort(404)

    current_difficulty = await rpc.call("current_difficulty", params=[])
    current_height = await rpc.call("current_height", params=[])
    hashrate = await rpc.call("get_hashrate", params=[])
    tx = await rpc.call("get_tx", params=[tx_hash])

    return await render_template(
        "tx.html",
        network=app.config["NETWORK"],
        current_difficulty=current_difficulty[0],
        current_height=current_height,
        hashrate=format_hashrate(hashrate),
        tx=tx,
    )


@app.route("/search")
async def search():
    """Search for blocks by height/hash or transactions by hash."""
    query = request.args.get("q", "").strip()

    if not query:
        return redirect(url_for("index"))

    # Try to interpret as block height (integer)
    if query.isdigit():
        return redirect(url_for("get_block_by_height", block_height=int(query)))

    # Check if it looks like a hex hash
    if all(c in '0123456789abcdefABCDEF' for c in query):
        # Use the search RPC to determine if it's a block or tx hash
        try:
            result = await rpc.call("search", params=[query])
            if result["type"] == "block":
                return redirect(f"/block/{result['height']}")
            elif result["type"] == "tx":
                return redirect(url_for("get_tx_by_hash", tx_hash=query))
        except JsonRpcError:
            pass

    # Nothing found
    return await render_template(
        "error.html",
        network=app.config["NETWORK"],
        error_code="Not Found",
        error=f"No block or transaction found for: {query}"
    ), 404


if __name__ == "__main__":
    app.run(host="127.0.0.1", port=5000, debug=True)
