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

import sys, toml, json, urwid, asyncio, logging

from model import Model
from rpc import JsonRpc
from view import View


class Dnetview:

    def __init__(self):
        self.ev = asyncio.new_event_loop()
        asyncio.set_event_loop(self.ev)
        self.queue = asyncio.Queue()
        self.config = self.get_config()
        self.model = Model()
        self.view = View(self.model)

    async def subscribe(self, rpc, name, host, port):
        info = {}
        while True:
            try:
                await rpc.start(host, port)
                logging.debug(f"Started {name} RPC on port {port}")
                break
            except Exception as e:
                logging.debug(f"failed to connect {host}:{port} {e}")
                pass
    
        data = await rpc._make_request("p2p.get_info", [])
        info[name] = data

        await self.queue.put(info)
        await rpc.dnet_switch(True)
        await rpc.dnet_subscribe_events()

        while True:
            await asyncio.sleep(0.01)
            data = await rpc.reader.readline()
            data = json.loads(data)
            info[name] = data
            await self.queue.put(info)

        await rpc.dnet_switch(False)
        await rpc.stop()
    
    def get_config(self):
        with open("config.toml") as f:
            cfg = toml.load(f)
            return cfg
    
    async def start_connect_slots(self, nodes):
        tasks = []
        async with asyncio.TaskGroup() as tg:
            for i, node in enumerate(nodes):
                rpc = JsonRpc()
                subscribe = tg.create_task(self.subscribe(
                            rpc, node['name'], node['host'],
                            node['port']))
                nodes = tg.create_task(self.update_info())

    async def update_info(self):
        while True:
            info = await self.queue.get()
            values = list(info.values())[0]
            method = values.get("method")

            if method == "dnet.subscribe_events":
                self.model.handle_event(info)
            else:
                self.model.handle_nodes(info)

            self.queue.task_done()

    def main(self):
        logging.basicConfig(filename='dnet.log',
                            encoding='utf-8',
                            level=logging.DEBUG)
        nodes = self.config.get("nodes")

        self.ev.create_task(self.start_connect_slots(nodes))
        self.ev.create_task(self.view.update_view())
        self.ev.create_task(self.view.render_info())

        loop = urwid.MainLoop(self.view.ui, self.view.palette,
                              unhandled_input=self.unhandled_input,
                              event_loop=urwid.AsyncioEventLoop(
                              loop=self.ev))
        loop.run()

    def unhandled_input(self, key):
        if key in ('q'):
            for task in asyncio.all_tasks():
                task.cancel()
            raise urwid.ExitMainLoop()
    

if __name__ == '__main__':
    dnet = Dnetview()
    dnet.main()
