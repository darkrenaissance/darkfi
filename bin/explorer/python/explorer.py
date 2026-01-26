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

from quart import Quart, render_template, abort, request, redirect, url_for, Response

from rpc_client import JsonRpcPool, JsonRpcError, RpcUnavailableError

app = Quart(__name__)
app.config.update(
    RPC_HOST="127.0.0.1",
    RPC_PORT="22222",
    RPC_MIN_CONNECTIONS=5,
    RPC_MAX_CONNECTIONS=50,
    RPC_RECONNECT_INTERVAL=5.0,
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
        reconnect_interval=app.config["RPC_RECONNECT_INTERVAL"],
    )
    await rpc.start()
    app.logger.info(f"RPC pool initialized for {app.config['RPC_HOST']}:{app.config['RPC_PORT']}")


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


@app.errorhandler(RpcUnavailableError)
async def handle_rpc_unavailable(error: RpcUnavailableError):
    app.logger.error(f"RPC Unavailable: {error}")
    return await render_template(
        "error.html",
        network=app.config["NETWORK"],
        error_code="503",
        error="Blockchain node is currently unavailable. Please try again later."
    ), 503


@app.errorhandler(ConnectionError)
async def handle_connection_error(error):
    app.logger.error(f"Connection Error: {error}")
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


def format_bytes(size: int) -> str:
    """Format byte size with appropriate unit."""
    if size >= 1024 * 1024:
        return f"{size / (1024 * 1024):.2f} MB"
    elif size >= 1024:
        return f"{size / 1024:.2f} KB"
    else:
        return f"{size} bytes"


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

    # Try as contract ID (base58)
    try:
        contract = await rpc.call("get_contract", params=[query])
        return redirect(url_for("get_contract", contract_id=query))
    except JsonRpcError:
        pass

    # Nothing found
    return await render_template(
        "error.html",
        network=app.config["NETWORK"],
        error_code="Not Found",
        error=f"No block, transaction, or contract found for: {query}"
    ), 404


@app.route("/contract/<contract_id>")
async def get_contract(contract_id: str):
    current_difficulty = await rpc.call("current_difficulty", params=[])
    current_height = await rpc.call("current_height", params=[])
    hashrate = await rpc.call("get_hashrate", params=[])
    contract = await rpc.call("get_contract", params=[contract_id])

    contract["wasm_size_formatted"] = format_bytes(int(contract["wasm_size"]))

    return await render_template(
        "contract.html",
        network=app.config["NETWORK"],
        current_difficulty=current_difficulty[0],
        current_height=current_height,
        hashrate=format_hashrate(hashrate),
        contract=contract,
    )


@app.route("/contracts")
async def list_contracts():
    current_difficulty = await rpc.call("current_difficulty", params=[])
    current_height = await rpc.call("current_height", params=[])
    hashrate = await rpc.call("get_hashrate", params=[])
    contracts = await rpc.call("list_contracts", params=[])
    contract_count = await rpc.call("contract_count", params=[])

    for contract in contracts:
        contract["wasm_size_formatted"] = format_bytes(int(contract["wasm_size"]))

    return await render_template(
        "contracts.html",
        network=app.config["NETWORK"],
        current_difficulty=current_difficulty[0],
        current_height=current_height,
        hashrate=format_hashrate(hashrate),
        contracts=contracts,
        contract_count=contract_count,
    )


@app.route("/stats")
async def stats():
    current_difficulty = await rpc.call("current_difficulty", params=[])
    current_height = await rpc.call("current_height", params=[])
    hashrate = await rpc.call("get_hashrate", params=[])
    stats_data = await rpc.call("get_stats", params=[])

    return await render_template(
        "stats.html",
        network=app.config["NETWORK"],
        current_difficulty=current_difficulty[0],
        current_height=current_height,
        hashrate=format_hashrate(hashrate),
        stats=stats_data,
    )


@app.route("/stats/daily_tx_chart.png")
async def daily_tx_chart():
    """Generate daily average transactions chart as PNG using matplotlib."""
    import io
    import matplotlib
    matplotlib.use('Agg')  # Non-interactive backend
    import matplotlib.pyplot as plt
    import matplotlib.dates as mdates

    stats_data = await rpc.call("get_stats", params=[])
    daily_stats = stats_data.get("daily_stats", [])

    # Filter to last 90 days
    if daily_stats:
        max_day = max(d["day"] for d in daily_stats)
        daily_stats = [d for d in daily_stats if d["day"] >= max_day - 90]

    # Create figure with dark theme
    plt.style.use('dark_background')
    fig, ax = plt.subplots(figsize=(12, 4), dpi=100)
    fig.patch.set_facecolor('#0d1117')
    ax.set_facecolor('#0d1117')

    if daily_stats:
        # Convert day numbers to dates
        dates = [datetime.fromtimestamp(d["day"] * 86400, tz=timezone.utc) for d in daily_stats]
        values = [d["avg_tx"] for d in daily_stats]

        ax.fill_between(dates, values, alpha=0.3, color='#6366f1')
        ax.plot(dates, values, color='#6366f1', linewidth=2)

        # Format x-axis
        ax.xaxis.set_major_formatter(mdates.DateFormatter('%b %d'))
        ax.xaxis.set_major_locator(mdates.DayLocator(interval=7))
        plt.xticks(rotation=45, ha='right')

        ax.set_ylabel('Avg TX per Block', color='#9ca3af')
        ax.tick_params(colors='#9ca3af')
        ax.spines['bottom'].set_color('#30363d')
        ax.spines['left'].set_color('#30363d')
        ax.spines['top'].set_visible(False)
        ax.spines['right'].set_visible(False)
        ax.grid(True, alpha=0.2, color='#30363d')
    else:
        ax.text(0.5, 0.5, 'No data available', ha='center', va='center',
                transform=ax.transAxes, color='#9ca3af', fontsize=14)
        ax.set_xlim(0, 1)
        ax.set_ylim(0, 1)

    plt.tight_layout()

    # Save to bytes buffer
    buf = io.BytesIO()
    plt.savefig(buf, format='png', facecolor='#0d1117', edgecolor='none')
    plt.close(fig)
    buf.seek(0)

    return Response(buf.getvalue(), mimetype='image/png')


if __name__ == "__main__":
    app.run(host="127.0.0.1", port=5000, debug=True)
