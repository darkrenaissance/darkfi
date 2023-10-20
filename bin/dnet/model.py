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
import datetime as dt


# -------------------------------------------------------------------
# TODO:
#   * on first get_info call, initialize data structure
#   * use channel id as key
#   * e.g. outbound[id] = [info1, info2, ...]
#   * create unique null id if not connected
# -------------------------------------------------------------------

class Model:

    def __init__(self):
        self.info = Info()
        self.nodes = {}

    def update_node(self, key, value):
        self.nodes[key] = value

    def handle_nodes(self, node):
        logging.debug(f"p2p_get_info(): {node}")
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

            id = channel["id"]
            url = channel_lookup[id]["url"]
            self.info.update_inbound(f"{id}", url)

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
        logging.debug(f"dnet_subscribe(): {event}")
        name = list(event.keys())[0]
        values = list(event.values())[0]
        params = values.get("params")
        event = params[0].get("event")
        info = params[0].get("info")

        t = time.localtime()
        current_time = time.strftime("%H:%M:%S", t)

        match event:                        
            case "send":
                nano = info.get("time")
                cmd = info.get("cmd")
                chan = info.get("chan")
                addr = chan.get("addr")
                t = (dt.datetime
                        .fromtimestamp(int(nano)/1000000000)
                        .strftime('%H:%M:%S'))
                self.info.update_msg(addr, (t, event, cmd))
            case "recv":
                nano = info.get("time")
                cmd = info.get("cmd")
                chan = info.get("chan")
                addr = chan.get("addr")
                t = (dt.datetime
                        .fromtimestamp(int(nano)/1000000000)
                        .strftime('%H:%M:%S'))
                self.info.update_msg(addr, (t, event, cmd))
            case "inbound_connected":
                addr = info["addr"]
                self.info.update_event((f"{name}", "inbound"), f"inbound (connect): {addr}")
            case "inbound_disconnected":
                addr = info["addr"]
                self.info.update_event((f"{name}","inbound"), f"inbound (disconnect): {addr}")
            case "outbound_slot_sleeping":
                slot = info["slot"]
                self.info.update_event((f"{name}", f"{slot}"), "sleeping")
            case "outbound_slot_connecting":
                slot = info["slot"]
                addr = info["addr"]
                self.info.update_event((f"{name}", f"{slot}"), f"connecting  addr={addr}")
            case "outbound_slot_connected":
                slot = info["slot"]
                addr = info["addr"]
                channel_id = info["channel_id"]
                self.info.update_event(f"{name}, {slot}", f"connected   addr={addr}")
            case "outbound_slot_disconnected":
                slot = info["slot"]
                err = info["err"]
                self.info.update_event((f"{slot}", "{slot}"), "disconnected")
            case "outbound_peer_discovery":
                attempt = info["attempt"]
                state = info["state"]
                self.info.update_event((f"{name}", "outbound"), f"peer discovery: {state} (attempt {attempt})")

    def __repr__(self):
        return f"{self.nodes}"
    

class Info:

    def __init__(self):
        self.outbound = {}
        self.inbound = {}
        self.manual = {}
        self.event = {}
        self.seed = {}
        self.msgs = {}
    
    def update_outbound(self, key, value):
        self.outbound[key] = value

    def update_inbound(self, key, value):
        self.inbound[key] = value

    def update_manual(self, key, value):
        self.manual[key] = value

    def update_seed(self, key, value):
        self.seed[key] = value

    def update_event(self, key, value):
        self.event[key] = value

    def update_msg(self, key, value):
        if key in self.msgs:
            self.msgs[key] += [value]
        else:
            self.msgs[key] = [value]

    def __repr__(self):
        return (f"outbound: {self.outbound}"
            f"inbound: {self.inbound}"
            f"manual: {self.manual}"
            f"seed: {self.seed}"
            f"msg: {self.msgs}")
