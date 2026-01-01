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
Module: app.py

This module initializes the DarkFi explorer Flask application by registering various blueprints for handling routes
related to blocks, contracts, transactions, search, and the explore section, including the home page. It also defines
error handlers, ensuring appropriate responses for these common HTTP errors.
"""

import os
import tomli

from flask import Flask, render_template

from blueprints.explore import explore_bp
from blueprints.block import block_bp
from blueprints.contract import contract_bp
from blueprints.transaction import transaction_bp

import log

def create_app():
    """
    Creates and configures the DarkFi explorer Flask application.

    This function creates and initializes the explorer the Flask app,
    registering applicable blueprints for handling explorer-related routes,
    and defining error handling for common HTTP errors. It returns a fully
    configured Flask application instance.
    """
    app = Flask(__name__)

    # Retrieve and store network
    network = os.getenv("FLASK_ENV", "localnet")
    app.config['NETWORK'] = network

    # Load the app TOML configuration
    load_toml_config(app, network)

    # Setup logger
    log.setup_logger(app, network)

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
        Handles 500 errors by logging the error and returning the app's 500 error page.

        This function logs the error with its stack trace, file name, and line number
        to help with debugging. It then renders the '500.html' template and returns it
        along with a 500 HTTP status code.

        Args:
            e: The error object associated with the 500 error.
        """
        # Log the error
        app.error_logger.exception("An unexpected error occurred")

        # Render the custom 500 error page
        return render_template('500.html'), 500

    # Log that we started the site
    app.logger.info("=" * 60)
    app.logger.info("Started Explorer Site")
    app.logger.info("=" * 60)
    app.logger.info(f"Network: {network}")
    app.logger.info(f"Explorer Node Endpoint: {app.config['explorer_rpc_url']}:{app.config['explorer_rpc_port']}")
    app.logger.info(f"Log Path: {app.config['log_path']}")
    app.logger.info("=" * 60)

    return app

def load_toml_config(app, network="localnet", config_path="site_config.toml"):
    """
    Loads environment-specific key-value pairs from a TOML configuration file into `app.config`.

    Args:
        app (Flask): The Flask application.
        network (str): The name of the network section to load (default is "localnet").
        config_path (str): The path to the TOML configuration file.

    Raises:
        FileNotFoundError: If the configuration file cannot be found.
        KeyError: If the specified environment section does not exist.
    """

    # Verify that the configuration file exists
    if not os.path.isfile(config_path):
        raise FileNotFoundError(f"Configuration file '{config_path}' not found.")

    # Open and parse the configuration file (TOML)
    with open(config_path, "rb") as f:
        config = tomli.load(f)

    # Ensure the specified network section exists in the configuration
    if network not in config:
        raise KeyError(f"Network '{network}' not found in {config_path}")

    # Load the environment specific configurations into app.config
    for key, value in config[network].items():
        app.config[key] = value
