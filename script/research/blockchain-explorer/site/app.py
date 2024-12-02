# This file is part of DarkFi (https://dark.fi)
#
# Copyright (C) 2020-2024 Dyne.org foundation
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

import rpc
from flask import Flask, request, render_template

app = Flask(__name__)

@app.route('/')
async def index():
    # Fetch data from RPC
    blocks = await rpc.get_last_n_blocks("10")
    basic_stats = await rpc.get_basic_statistics()

    # Fetch the metric statistics
    metric_stats = await rpc.get_metric_statistics()
    has_metrics = metric_stats and isinstance(metric_stats, list)

    # Get the latest metric statistics, or use None if no metrics are found
    if has_metrics:
        latest_metric_stats = metric_stats[-1]
    else:
        latest_metric_stats = None

    # Render template
    return render_template(
        'index.html',
        blocks=blocks,
        basic_stats=basic_stats,
        metric_stats=latest_metric_stats,
    )

@app.route('/search', methods=['GET', 'POST'])
async def search():
    search_hash = request.args.get('search_hash', '')
    block = await rpc.get_block(search_hash)
    transactions = await rpc.get_block_transactions(search_hash)
    if transactions:
        return render_template('block.html', block=block, transactions=transactions)
    else:
        transaction = await rpc.get_transaction(search_hash)
        return render_template('transaction.html', transaction=transaction)

@app.route('/block/<header_hash>')
async def block(header_hash):
    block = await rpc.get_block(header_hash)
    transactions = await rpc.get_block_transactions(header_hash)
    return render_template('block.html', block=block, transactions=transactions)


@app.route('/transaction/<transaction_hash>')
async def transaction(transaction_hash):
    transaction = await rpc.get_transaction(transaction_hash)
    return render_template('transaction.html', transaction=transaction)

@app.errorhandler(404)
def page_not_found(e):
    return render_template('404.html'), 404

@app.errorhandler(500)
def page_not_found(e):
    return render_template('500.html'), 500

