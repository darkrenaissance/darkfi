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
        self.ev = asyncio.get_event_loop()
        self.queue = asyncio.Queue()

        self.config = self.get_config()
        self.model = Model()
        self.view = View(self.model)

    async def subscribe(self, rpc, name, port):
        info = {}
    
        while True:
            try:
                logging.debug(f"Start {name} RPC on port {port}")
                await rpc.start("localhost", port)
                break
            # TODO: offline node handling
            except OSError:
                pass
    
        data = await rpc._make_request("p2p.get_info", [])
        info[name] = data

        try:
            self.queue.put_nowait(info)
        except:
            logging.debug("subscribe().put_nowait(): QueueFull")

        await rpc.dnet_switch(True)
        await rpc.dnet_subscribe_events()

        while True:
            data = await rpc.reader.readline()
            data = json.loads(data)
            info[name] = data
            #logging.debug(f"events: {data}")

            try:
                self.queue.put_nowait(info)
            except:
                logging.debug("subscribe().putnowait(): QueueFull")
    
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
                subscribe = tg.create_task(self.subscribe(rpc, node['name'], node['port']))
                nodes = tg.create_task(self.update_info())

    async def update_info(self):
        while True:
            try:
                info = await self.queue.get()
                values = list(info.values())[0]

                # Update node info
                if "result" in values:
                    self.model.update(info)

                # Update event info: TODO
                if "params" in values:
                    logging.debug("update_info(): Event detected")

                self.queue.task_done()
            except self.queue.is_empty():
                logging.debug("update_model(): QueueEmpty")

    def main(self):
        logging.basicConfig(filename='dnet.log', encoding='utf-8', level=logging.DEBUG)
        nodes = self.config.get("nodes")

        self.ev.create_task(self.start_connect_slots(nodes))
        self.ev.create_task(self.view.update_view(self.model))

        loop = urwid.MainLoop(self.view.ui, self.view.palette,
            unhandled_input=self.unhandled_input,
            event_loop=urwid.AsyncioEventLoop(loop=self.ev))

        #loop.set_alarm_in(2, self.view.update_view)
        loop.run()

    def unhandled_input(self, key):
        if key in ('q'):
            for task in asyncio.all_tasks():
                task.cancel()
            raise urwid.ExitMainLoop()
    
if __name__ == '__main__':
    dnet = Dnetview()
    dnet.main()
