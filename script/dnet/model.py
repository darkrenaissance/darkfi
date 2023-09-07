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
        self.nodes = {}

    def update(self, new_node):
        self.nodes.update(new_node)

    def __repr__(self):
        return f"{self.nodes}"

class NodeInfo():
    def __init__(self, channels, slots):
        self.node = {}
        inbound = {}
        outbounds = {"slots": []}
        manual = {}
        seed = {}

        for name, channels in info.items():
            channel_lookup = {}
            for channel in channels:
                id = channel["id"]
                channel_lookup[id] = channel

            for channel in channels:
                if channel["session"] != "inbound":
                    continue
                url = channel["url"]
                inbound["inbound"] = url

            
            for i, id in enumerate(slots):
                if id == 0:
                    outbounds["slots"].append(f"{i}: none")
                    continue

                assert id in channel_lookup
                url = channel_lookup[id]["url"]
                outbounds["slots"].append(f"{i}: {url}")

            for channel in channels:
                if channel["session"] != "seed":
                    continue
                url = channel["url"]
                seed["seed"] = url

            for channel in channels:
                if channel["session"] != "manual":
                    continue
                url = channel["url"]
                manual["manual"] = url

        self.node[name] = [inbound, outbounds, manual,
                                seed]

    def __repr__(self):
        return f"{self.node}"


