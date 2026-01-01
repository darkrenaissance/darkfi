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
Explorer blueprint package initializer.

This module imports and exposes blueprints for used by the explorer
flask application.

Exposed Blueprints:
    - explore_bp: The explorer application blueprint.
    - block_bp: The blueprint for block-related functionality.
    - contract_bp: The blueprint for contract-related functionality.
    - transaction_bp: The blueprint for transaction-related functionality.
"""
from .explore import explore_bp
from .block import block_bp
from .contract import contract_bp
from .transaction import transaction_bp

# Expose blueprints for importing
__all__ = ["explore_bp", "block_bp", "contract_bp", "transaction_bp"]
