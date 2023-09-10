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

import logging

class Model:
    def __init__(self):
        self.info = Info()
        self.nodes = {}

    def update_node(self, key, value):
        self.nodes[key] = value

    def handle_nodes(self, node):
        channel_lookup = {}

        name = list(node.keys())[0]
        values = list(node.values())[0]

        info = values["result"]
        channels = info["channels"]

        for channel in channels:
            id = channel["id"]
            channel_lookup[id] = channel

        for channel in channels:
            if channel["session"] != "inbound":
                continue
            url = channel["url"]
            self.info.update_inbound("inbound", url)

        for i, id in enumerate(info["outbound_slots"]):
            if id == 0:
                self.info.update_outbound(f"{i}", "none")
                continue

            assert id in channel_lookup
            url = channel_lookup[id]["url"]
            self.info.update_outbound(f"{i}", url)

        for channel in channels:
            if channel["session"] != "seed":
                continue
            url = channel["url"]
            self.info.update_seed("seed", url)

        for channel in channels:
            if channel["session"] != "manual":
                continue
            url = channel["url"]
            self.info.update_manual("manual", url)

        self.update_node(name, self.info)

    def handle_event(self, event):
        name = list(event.keys())[0]
        values = list(event.values())[0]

        params = values.get("params")
        event = params[0].get("event")
        info = params[0].get("info")

        if "chan" in info:
            time = info.get("time")
            cmd = info.get("cmd")
            chan = info.get("chan")
            addr = chan.get("addr")

            self.info.update_msg(addr, (time, event, cmd))
        else:
            # TODO
            #slot = info.get("slot")
            logging.debug(info)
            logging.debug(event)
            #self.info.update_msgs(addr, (time, event, cmd))

    def __repr__(self):
        return f"{self.nodes}"
    
class Info:
    def __init__(self):
        self.outbounds = {}
        self.inbound = {}
        self.manual = {}
        self.seed = {}
        self.msgs = {}
    
    def update_outbound(self, key, value):
        self.outbounds[key] = value

    def update_inbound(self, key, value):
        self.inbound[key] = value

    def update_manual(self, key, value):
        self.manual[key] = value

    def update_seed(self, key, value):
        self.seed[key] = value

    def update_msg(self, key, value):
        if key in self.msgs:
            self.msgs[key] += [value]
        else:
            self.msgs[key] = [value]

    def __repr__(self):
        return (
            f"outbound: {self.outbounds}"
            f"inbound: {self.inbound}"
            f"manual: {self.manual}"
            f"seed: {self.seed}"
            f"msg: {self.msgs}"
            )
