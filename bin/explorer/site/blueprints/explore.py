# This file is part of DarkFi (https://dark.fi)
#
# Copyright (C) 2020-2025 Dyne.org foundation
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

"""
Blueprint: explorer_bp

This module defines a Flask blueprint (`explore_bp`) for managing the general functionality of the explorer application.
It serves as the primary location for Flask routes related to the home page, search functionality, and other general features.
"""

from flask import Blueprint, render_template, request

import rpc

# Create explore blueprint
explore_bp = Blueprint("explore", __name__)
@explore_bp.route('/', methods=["GET"])
async def index():
    """
    Fetches and displays the explorer home page content using multiple RPC calls to the
    explorer daemon, retrieving the last 10 blocks, basic statistics, metric statistics,
    and DarkFi native contracts.

    Upon success, it returns a rendered template with recent blocks, basic statistics,
    latest metric statistics (if available), and native contracts.
    """
    # Fetch the latest 10 blocks
    blocks = await rpc.get_last_n_blocks(10)

    # Retrieve basic statistics summarizing the overall chain data
    basic_stats = await rpc.get_basic_statistics()

    # Fetch the metric statistics
    metric_stats = await rpc.get_metric_statistics()

    # Determine if metrics exist
    has_metrics = metric_stats and isinstance(metric_stats, list)

    # Get the latest metric statistics or return empty metrics
    latest_metric_stats = metric_stats[-1] if has_metrics else [0] * 15

    # Retrieve the native contracts
    native_contracts = await rpc.get_native_contracts()

    # Render the explorer home page
    return render_template(
        'index.html',
        blocks=blocks,
        basic_stats=basic_stats,
        metric_stats=latest_metric_stats,
        native_contracts=native_contracts,
    )

@explore_bp.route('/search', methods=['GET', 'POST'])
async def search():
    """
    Searches for a block or transaction based on the provided hash using RPC calls to the
    explorer daemon. It retrieves relevant data using the search hash from provided query parameter,
    returning a rendered template that displays either block details with associated transactions
    or transaction details, depending on the search result.

    Query Params:
        search_hash (str): The hash of the block or transaction to search for.
    """
    # Get the search hash
    search_hash = request.args.get('search_hash', '')

    # Fetch the block corresponding to the search hash
    block = await rpc.get_block(search_hash)

    # Fetch transactions associated with the block
    transactions = await rpc.get_block_transactions(search_hash)

    if transactions:
        # Render block details with associated transactions if found
        return render_template('block.html', block=block, transactions=transactions)
    else:
        # Fetch transaction details if no transactions are found for the block
        transaction = await rpc.get_transaction(search_hash)
        return render_template('transaction.html', transaction=transaction)