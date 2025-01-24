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
Module: app.py

This module initializes the DarkFi explorer Flask application by registering various blueprints for handling routes
related to blocks, contracts, transactions, search, and the explore section, including the home page. It also defines
error handlers, ensuring appropriate responses for these common HTTP errors.
"""

from flask import Flask, render_template

from blueprints.explore import explore_bp
from blueprints.block import block_bp
from blueprints.contract import contract_bp
from blueprints.transaction import transaction_bp

def create_app():
    """
    Creates and configures the DarkFi explorer Flask application.

    This function creates and initializes the explorer the Flask app,
    registering applicable blueprints for handling explorer-related routes,
    and defining error handling for common HTTP errors. It returns a fully
    configured Flask application instance.
    """
    app = Flask(__name__)

    # Register Blueprints
    app.register_blueprint(explore_bp)
    app.register_blueprint(block_bp)
    app.register_blueprint(contract_bp)
    app.register_blueprint(transaction_bp)

    # Define page not found error handler
    @app.errorhandler(404)
    def page_not_found(e):
        """
        Handles 404 errors by rendering a custom 404 error page when a requested page is not found,
        returning a rendered template along with a 404 status code.

        Args:
            e: The error object associated with the 404 error.
        """
        # Render the custom 404 error page
        return render_template('404.html'), 404

    # Define internal server error handler
    @app.errorhandler(500)
    def internal_server_error(e):
        """
        Handles 500 errors by rendering a custom 500 error page when an internal server error occurs,
        returning a rendered template along with a 500 status code.

        Args:
            e: The error object associated with the 500 error.
        """
        # Render the custom 500 error page
        return render_template('500.html'), 500

    return app
