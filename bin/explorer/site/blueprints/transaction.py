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
Blueprint: transaction_bp

This module defines a Flask blueprint (`transaction_bp`) for handling transaction-related functionality,
serving as a primary location for Flask code related to routes and related features associated with transactions.
"""

from flask import Blueprint, render_template

import rpc

# Create transaction blueprint
transaction_bp = Blueprint("transaction", __name__)

@transaction_bp.route('/tx/<transaction_hash>')
async def transaction(transaction_hash):
    """
    Retrieves transaction details based on the provided hash using RPC calls to the explorer daemon
    and returns a rendered template displaying the information.

    Path Args:
        transaction_hash (str): The hash of the transaction to retrieve.
    """
    # Fetch the transaction details
    transaction = await rpc.get_transaction(transaction_hash)

    # Render the template using the fetched transaction details
    return render_template('transaction.html', transaction=transaction)
