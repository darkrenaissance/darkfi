# This file is part of DarkFi (https://dark.fi)
#
# Copyright (C) 2020-2023 Dyne.org foundation
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

import sys
import urwid
import asyncio
import logging

import model
from rpc import JsonRpc
from view import Dnetview

async def get_info(rpc):
    while True:
        try:
            await rpc.start("localhost", 26660)
            break
        except OSError:
            pass
    response = await rpc._make_request("p2p.get_info", [])
    info = response["result"]
    channels = info["channels"]
    channel_lookup = {}
    for channel in channels:
        id = channel["id"]
        channel_lookup[id] = channel

    logging.debug("inbound")
    for channel in channels:
        if channel["session"] != "inbound":
            continue
        url = channel["url"]
        logging.debug(f"  {url}")

    logging.debug("outbound")
    for i, id in enumerate(info["outbound_slots"]):
        if id == 0:
            logging.debug(f"  {i}: none")
            continue

        assert id in channel_lookup
        url = channel_lookup[id]["url"]
        logging.debug(f"  {i}: {url}")

    logging.debug("seed")
    for channel in channels:
        if channel["session"] != "seed":
            continue
        url = channel["url"]
        logging.debug(f"  {i}: {url}")

    logging.debug("manual")
    for channel in channels:
        if channel["session"] != "manual":
            continue
        url = channel["url"]
        logging.debug(f"  {i}: {url}")

    await rpc.stop()

if __name__ == '__main__':
    logging.basicConfig(filename='dnet.log', encoding='utf-8', level=logging.DEBUG)

    ev = asyncio.get_event_loop()

    rpc = JsonRpc()
    ev.create_task(get_info(rpc))

    dnet = Dnetview()
    ev.create_task(dnet.render_info())

    loop = urwid.MainLoop(dnet.view, dnet.palette,
        event_loop=urwid.AsyncioEventLoop(loop=ev))
    loop.run()
