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

import logging, time 


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
            t = info.get("time")
            cmd = info.get("cmd")
            chan = info.get("chan")
            addr = chan.get("addr")

            self.info.update_msg(addr, (t, event, cmd))
        else:
            t = time.localtime()
            current_time = time.strftime("%H:%M:%S", t)
            logging.debug(current_time)

            match event:
                case "inbound_connected":
                    addr = info["addr"]
                    logging.debug(f"{current_time} inbound (connect):    {addr}")
                case "inbound_disconnected":
                    addr = info["addr"]
                    logging.debug(f"{current_time} inbound (disconnect): {addr}")
                case "outbound_slot_sleeping":
                    slot = info["slot"]
                    logging.debug(f"{current_time} slot {slot}: sleeping")
                case "outbound_slot_connecting":
                    slot = info["slot"]
                    addr = info["addr"]
                    logging.debug(f"{current_time} slot {slot}: connecting   addr={addr}")
                case "outbound_slot_connected":
                    slot = info["slot"]
                    addr = info["addr"]
                    channel_id = info["channel_id"]
                    logging.debug(f"{current_time} slot {slot}: connected    addr={addr}")
                case "outbound_slot_disconnected":
                    slot = info["slot"]
                    err = info["err"]
                    logging.debug(f"{current_time} slot {slot}: disconnected err='{err}'")
                case "outbound_peer_discovery":
                    attempt = info["attempt"]
                    state = info["state"]
                    logging.debug(f"{current_time} peer_discovery: {state} (attempt {attempt})")

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
        return (f"outbound: {self.outbounds}"
            f"inbound: {self.inbound}"
            f"manual: {self.manual}"
            f"seed: {self.seed}"
            f"msg: {self.msgs}")
