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

import os
import logging

from logging.handlers import RotatingFileHandler

"""
Module: log.py

This module provides functionality to setup logging for the explorer Flask application.
"""

def setup_logger(app, env):
    """
    Sets up logging for the explorer Flask app by setting up error, application, and request logging.

    The error logger captures application errors and logs them to a dedicated error log file. The application
    logger handles general application-level logs such as debug or informational messages. Additionally, the
    request logger, derived from the Werkzeug logger, manages HTTP request logs and directs them to the
    same file as the application logger.

    The overall log level is determined by the LOG_LEVEL environment variable, defaulting to INFO if the
    variable is not set or contains an invalid value. The path where logs are stored is obtained from the
    application's TOML configuration file under the 'log_path' entry. If not specified, it defaults to the
    current directory.

    Args:
        app (Flask): The Flask application instance.
        env (str): The environment (e.g., 'localnet', 'mainnet', 'testnet', etc.).
    """
    log_path = app.config.get('log_path', '.')

    # Expand the path if home directory is specified
    log_path = os.path.expanduser(log_path)

    # Ensure the log path exists or create it if not
    if not os.path.exists(log_path):
        try:
            os.makedirs(log_path)
            print(f"created log dir: {log_path}")
        except OSError as e:
            raise RuntimeError(f"Unable to create log directory at '{log_path}': {e}")

    # Get log level from environment variable, default to INFO
    log_level_name = os.environ.get('LOG_LEVEL', 'INFO').upper()
    try:
        log_level = getattr(logging, log_level_name)
    except AttributeError:
        if log_level_name:
            app.logger.warning(f"Invalid LOG_LEVEL '{log_level_name}'. Defaulting to INFO.")
        log_level = logging.INFO

    # App logger setup
    app_logger = setup_app_logger(log_path, env, log_level)
    app.logger = app_logger

    # Request logger setup
    app_log_file = os.path.join(log_path, 'app.log')
    setup_request_logger(app_log_file, env, log_level)

    # Error logger setup
    error_logger = setup_error_logger(log_path, env)
    app.error_logger = error_logger

def setup_error_logger(log_path, env):
    """
    Configures the error logger to capture application errors, returning an error logger instance.

    Args:
        log_path (str): Path to the directory where logs are stored.
        env (str): The application environment.

    Returns:
        logging.Logger: Configured error logger instance.
    """
    error_logger = logging.getLogger('error_logger')
    error_log_file = os.path.join(log_path, 'error.log')
    error_handler = initialize_log_handler(error_log_file, env)
    error_handler.setLevel(logging.ERROR)
    error_formatter = logging.Formatter('%(asctime)s %(levelname)s: %(message)s [in %(pathname)s:%(lineno)d]')
    error_handler.setFormatter(error_formatter)
    error_logger.addHandler(error_handler)
    error_logger.setLevel(logging.ERROR)
    error_logger.propagate = False

    add_console_handler_if_localnet(env, error_logger, logging.ERROR)

    return error_logger

def setup_app_logger(log_path, env, log_level=logging.INFO):
    """
    Configures the app logger for general application logging, returning an app logger instance.

    Args:
        log_path (str): Path to the directory where logs are stored.
        env (str): The application environment.
        log_level (int): The logging level (default is INFO).
    """
    app_logger = logging.getLogger('app_logger')
    app_log_file = os.path.join(log_path, 'app.log')
    app_handler = initialize_log_handler(app_log_file, env)
    app_handler.setLevel(log_level)
    app_formatter = logging.Formatter('%(asctime)s %(message)s')
    app_handler.setFormatter(app_formatter)
    app_logger.addHandler(app_handler)
    app_logger.setLevel(log_level)
    app_logger.propagate = False

    add_console_handler_if_localnet(env, app_logger, log_level)

    return app_logger

def setup_request_logger(log_file, env, log_level=logging.INFO):
    """
    Configures the request logger to handle HTTP request logs based on the specified environment.

    If the environment is set to 'localnet', HTTP requests are logged to the console to facilitate
    local development and debugging. For all other environments, such as 'testnet' or 'mainnet',
    HTTP requests are logged to the specified log file, ensuring that logs are persisted in a location
    appropriate for testing or production use.

    Args:
        log_file (str): Path to the log file where HTTP requests should be logged.
        env (str): The application environment (e.g., 'localnet', 'testnet', 'mainnet').
        log_level (int): The logging level (default is INFO).
    """
    # Get the werkzeug logger that logs requests
    request_logger = logging.getLogger('werkzeug')
    request_logger.setLevel(log_level)
    request_logger.propagate = False

    file_handler = logging.FileHandler(log_file)
    file_handler.setLevel(log_level)
    request_logger.addHandler(file_handler)

    add_console_handler_if_localnet(env, request_logger, log_level)

def initialize_log_handler(log_file, env):
    """
    Initializes and returns a log handler based on the environment.

    Args:
        log_file (str): Path to the log file.
        env (str): The environment (e.g., 'mainnet', 'testnet', etc.).
    """
    if env == "mainnet":
        return RotatingFileHandler(log_file, maxBytes=100_000_000, backupCount=5)
    else:
        return logging.FileHandler(log_file)

def add_console_handler_if_localnet(env, logger, log_level=logging.INFO):
    """
    Adds a console handler to the given logger if the environment is 'localnet'.

    Args:
    env (str): The current environment (e.g., 'localnet', 'mainnet').
    logger (logging.Logger): The logger to which the console handler should be added.
    log_level (int): The logging level for the console handler.
    """

    # If localnet, also log to console
    if env == 'localnet':
        console_handler = logging.StreamHandler()
        console_handler.setLevel(log_level)
        formatter = logging.Formatter('%(asctime)s - %(levelname)s - %(message)s')
        console_handler.setFormatter(formatter)
        logger.addHandler(console_handler)
