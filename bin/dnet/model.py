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
from collections import defaultdict as dd


class Model:

    def __init__(self):
        self.nodes = {}

    def add_node(self, node):
        channel_lookup = {}
        name = list(node.keys())[0]
        values = list(node.values())[0]
        info = values["result"]
        channels = info["channels"]
        
        self.nodes[name] = {}
        self.nodes[name]['outbound'] = {}
        self.nodes[name]['inbound'] = {}
        self.nodes[name]['manual'] = {}
        self.nodes[name]['event'] = {}
        self.nodes[name]['seed'] = {}
        self.nodes[name]['msgs'] = dd(list)

        for channel in channels:
            id = channel["id"]
            channel_lookup[id] = channel

        for channel in channels:
            if channel["session"] != "inbound":
                continue
            id = channel["id"]
            url = channel_lookup[id]["url"]
            self.nodes[name]['inbound'][f"{id}"] = url

        for i, id in enumerate(info["outbound_slots"]):
            if id == 0:
                outbounds = self.nodes[name]['outbound'][f"{i}"] = ["none", 0]
                continue
            assert id in channel_lookup
            url = channel_lookup[id]["url"]
            outbounds = self.nodes[name]['outbound'][f"{i}"] = [url, id]

        for channel in channels:
            if channel["session"] != "seed":
                continue
            id = channel["id"]
            url = channel["url"]
            self.nodes[name]['seed'][f"{id}"] = url

        for channel in channels:
            if channel["session"] != "manual":
                continue
            id = channel["id"]
            url = channel["url"]
            self.nodes[name]['manual'][f"{id}"] = url
    
    def add_offline(self, node):
        name = list(node.keys())[0]
        values = list(node.values())[0]
        self.nodes[name] = values

    def add_event(self, event):
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
                msgs = self.nodes[name]['msgs']
                msgs[addr].append((t, event, cmd))
            case "recv":
                nano = info.get("time")
                cmd = info.get("cmd")
                chan = info.get("chan")
                addr = chan.get("addr")
                t = (dt.datetime
                        .fromtimestamp(int(nano)/1000000000)
                        .strftime('%H:%M:%S'))
                msgs = self.nodes[name]['msgs']
                msgs[addr].append((t, event, cmd))
            case "inbound_connected":
                addr = info["addr"]
                id = info.get("channel_id")
                self.nodes[name]['inbound'][f"{id}"] = addr
                logging.debug(f"{current_time}  inbound (connect):    {addr}")
            case "inbound_disconnected":
                addr = info["addr"]
                id = info.get("channel_id")
                inbound = self.nodes[name]['inbound']
                self.nodes[name]['inbound'][f"{id}"] = {}
                logging.debug(f"{current_time}  inbound (disconnect): {addr}")
            case "outbound_slot_sleeping":
                slot = info["slot"]
                event = self.nodes[name]['event']
                event[(f"{name}", f"{slot}")] = ["sleeping", 0]
                logging.debug(f"{current_time}  slot {slot}: sleeping")
            case "outbound_slot_connecting":
                slot = info["slot"]
                addr = info["addr"]
                event = self.nodes[name]['event']
                event[(f"{name}", f"{slot}")] = [f"connecting: addr={addr}", 0]
                logging.debug(f"{current_time}  slot {slot}: connecting   addr={addr}")
            case "outbound_slot_connected":
                slot = info["slot"]
                addr = info["addr"]
                id = info["channel_id"]
                self.nodes[name]['outbound'][f"{slot}"] = [addr, id]
                logging.debug(f"{current_time}  slot {slot}: connected    addr={addr}")
            case "outbound_slot_disconnected":
                slot = info["slot"]
                err = info["err"]
                event = self.nodes[name]['event']
                event[(f"{name}", f"{slot}")] = [f"disconnected: {err}", 0]
                logging.debug(f"{current_time}  slot {slot}: disconnected err='{err}'")
            case "outbound_peer_discovery":
                attempt = info["attempt"]
                state = info["state"]
                event = self.nodes[name]['event']
                key = (f"{name}", "outbound")
                event[key] = f"peer discovery: {state} (attempt {attempt})"
                logging.debug(f"{current_time}  peer_discovery: {state} (attempt {attempt})")


    def __repr__(self):
        return f"{self.nodes}"
