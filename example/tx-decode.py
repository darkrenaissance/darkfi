#!/usr/bin/env python

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

import argparse, sys, base64, json
from darkfi_sdk.tx import Transaction

def main(argv):
    parser = argparse.ArgumentParser(description='Tool to decode Darkfid base64 transaction')
    parser.add_argument('--format', default='info', help='Output format. options are info and json, defaults to info')
    parser.add_argument("tx", nargs="?", help="The base64 transaction data")

    args = parser.parse_args()

    if args.tx:
        base64_tx = args.tx
    else:
        base64_tx = sys.stdin.read().strip()

    tx_bytes = base64.b64decode(base64_tx)
    tx = Transaction.decode(tx_bytes)
    if args.format == 'json':
        print(json.dumps(tx.__dict__, indent=4))
    else:
        print(tx)

main(sys.argv)
