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

"""
Blueprint: block_bp

This module defines a Flask blueprint (`block_bp`) for handling block-related functionality,
serving as a primary location for Flask code related to routes and features associated with blocks.
"""

from flask import Blueprint, render_template

import rpc

# Create block blueprint
block_bp = Blueprint("block", __name__)

@block_bp.route('/block/<header_hash>')
async def block(header_hash):
    """
    Retrieves and displays details of a specific block and its associated transactions based
    on the provided header hash using RPC calls to the explorer daemon, returning a rendered template.

    Path Args:
        header_hash (str): The header hash of the block to retrieve.
    """
    # Fetch the block details
    block = await rpc.get_block(header_hash)

    # Fetch transactions associated with the block
    transactions = await rpc.get_block_transactions(header_hash)

    # Render the template with the block details and associated transactions
    return render_template('block.html', block=block, transactions=transactions)
