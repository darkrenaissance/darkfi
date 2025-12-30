#!/usr/bin/env python3

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

import asyncio
import sys
from node_get_info import JsonRpc

async def main(argv):
    if len(argv) != 2:
        print(f"Usage: {argv[0]} <num_connections>", file=sys.stderr)
        sys.exit(1)

    try:
        num_conns = int(argv[1])
        if num_conns <= 0:
            raise ValueError()
    except ValueError:
        print("Error: num_connections must be a positive integer", file=sys.stderr)
        sys.exit(1)

    rpc = JsonRpc()
    while True:
        try:
            await rpc.start("localhost", 26660)
            break
        except OSError:
            pass

    response = await rpc.set_outbound_connections(num_conns)

    if "error" in response:
        print(f"Error: {response['error']}")
        await rpc.stop()
        sys.exit(1)

    print(f"Set outbound connections to {num_conns}: {response['result']}")
    await rpc.stop()

asyncio.run(main(sys.argv))
