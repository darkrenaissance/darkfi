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

"""
Blueprint: contract_bp

This module defines a Flask blueprint (`contract_bp`) for handling contract-related functionality,
serving as a primary location for Flask code related to routes and related features associated with contracts.
"""

from flask import request, render_template, Blueprint

from pygments import highlight
from pygments.lexers import RustLexer
from pygments.formatters import HtmlFormatter

import rpc

# Create contract blueprint
contract_bp = Blueprint("contract", __name__)

@contract_bp.route('/contract/<contract_id>')
async def contract_source_list(contract_id):
    """
    Fetches and displays a list of source files for the specified contract using an RPC
    call to the explorer daemon, returning a rendered template with the source code
    files associated with the contract.

    Args:
        contract_id (str): The Contract ID to fetch the source files for.

    Query Params:
        name (str, optional): The contract name to display alongside the source files.
    """
    # Obtain the contract name to display with the source
    contract_name = request.args.get('name')

    # Fetch the source file list associated with the contract
    source_paths = await rpc.get_contract_source_paths(contract_id)

    # Returned rendered contract source list
    return render_template('contract_source_list.html', contract_id=contract_id, source_paths=source_paths, contract_name=contract_name)

@contract_bp.route('/contract/source/<contract_id>/<path:source_path>')
async def contract_source(contract_id, source_path):
    """
    Fetches and displays the source code for a specific file of a contract using an RPC
    call to the explorer daemon, returning a rendered template with syntax-highlighted
    source code.

    Path Args:
        contract_id (str): The Contract ID to fetch the source file for.
        source_path (str): The path of the specific source file within the contract.

    Query Params:
        name (str, optional): The contract name to display alongside the source code.
    """
    # Obtain the contract name to display with the source
    contract_name = request.args.get('name')

    # Retrieve the contract source code
    raw_source = await rpc.get_contract_source(contract_id, source_path)

    # Style the source code
    formatter = HtmlFormatter(style='friendly', linenos=True)
    source = highlight(raw_source, RustLexer(), formatter)

    # Generate css for styled source code
    pygments_css = formatter.get_style_defs()

    # Returned rendered contract source code page
    return render_template(
        'contract_source.html',
        source=source,
        contract_id=contract_id,
        source_path=source_path,
        pygments_css=pygments_css,
        contract_name=contract_name
    )

